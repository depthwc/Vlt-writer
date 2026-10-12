//! Turn a photo of ink on paper into an ink alpha matte that keeps the real pen texture.

use rayon::prelude::*;

use crate::img::{self, Gray, Rgb};
use crate::signal;

/// Paper brightness without ink: closing (max then min) removes thin dark strokes, blur smooths.
/// The result is smooth, so it is computed at ~5 px/mm and scaled back up (much faster).
pub fn background(ch: &Gray, ppm: f32) -> Gray {
    let (small, f) = background_small(ch, ppm);
    if f > 1 { small.upsample(ch.w, ch.h).map(|v| v.max(1e-3)) } else { small }
}

/// Background at reduced resolution and the reduction factor.
fn background_small(ch: &Gray, ppm: f32) -> (Gray, usize) {
    let f = ((ppm / 5.0).floor() as usize).max(1);
    if f > 1 && ch.w / f > 16 && ch.h / f > 16 {
        (background_at(&ch.downsample(f), ppm / f as f32), f)
    } else {
        (background_at(ch, ppm), 1)
    }
}

fn background_at(ch: &Gray, ppm: f32) -> Gray {
    let k = ((2.5 * ppm) as usize).max(3) | 1;
    let closed = img::minmax(&img::minmax(ch, k, true), k, false);
    img::blur(&closed, k as f32).map(|v| v.max(1e-3))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// Our sample sheet: red channel, where the pastel guides vanish.
    Sheet,
    /// Any photo: darkest channel, then strip ruled lines.
    Generic,
}

/// Mask minus non-handwriting: big solid blocks (markers) and long thin rules.
fn handwriting(mask: &[bool], w: usize, h: usize, ppm: f32) -> Vec<bool> {
    let (lab, comps) = img::components(mask, w, h);
    let bad: Vec<bool> = comps
        .iter()
        .enumerate()
        .map(|(i, c)| {
            if i == 0 {
                return true;
            }
            let (cw, ch) = (c.w() as f32, c.h() as f32);
            (cw > 12.0 * ppm && ch > 12.0 * ppm) || (cw.max(ch) > 40.0 * ppm && cw.min(ch) < 1.2 * ppm)
        })
        .collect();
    lab.iter().map(|&l| !bad[l as usize]).collect()
}

/// Erase long thin horizontal/vertical printed lines; strokes crossing them are bridged by
/// interpolating across the line.
pub fn remove_rules(alpha: &mut Gray, ppm: f32) {
    remove_lines(alpha, ppm, None)
}

/// Pixels of printed lines: long horizontal and vertical runs (a little slanted allowed, and
/// faint stretches bridged). Returns (horizontal, vertical) masks of the line pixels themselves.
fn line_masks(alpha: &Gray, ppm: f32) -> (Vec<bool>, Vec<bool>) {
    let (w, h) = (alpha.w, alpha.h);
    let min_run = (12.0 * ppm) as usize;
    let on: Vec<bool> = alpha.d.iter().map(|&v| v > 0.1).collect();
    // a printed line seen at a slight angle steps from one pixel row (column) to the next:
    // look for long runs a little across it (0.2 mm each side)
    let r = ((0.2 * ppm).round() as usize).max(1);
    let near_h: Vec<bool> = (0..w * h).map(|i| {
        let (x, y) = (i % w, i / w);
        (y.saturating_sub(r)..(y + r + 1).min(h)).any(|yy| on[yy * w + x])
    }).collect();
    let near_v: Vec<bool> = (0..w * h).map(|i| {
        let (x, y) = (i % w, i / w);
        (x.saturating_sub(r)..(x + r + 1).min(w)).any(|xx| on[y * w + xx])
    }).collect();
    // runs along one row or column, going over faint gaps up to 0.6 mm (a line printed or
    // photographed unevenly comes out dashed)
    let gap = ((0.6 * ppm).round() as usize).max(2);
    let runs = |n: usize, gap: usize, at: &dyn Fn(usize) -> bool| -> Vec<(usize, usize)> {
        let mut out = vec![];
        let mut start: Option<usize> = None;
        let mut last = 0;
        for k in 0..n {
            if at(k) {
                if start.is_none() {
                    start = Some(k);
                }
                last = k;
            } else if let Some(s0) = start {
                if k - last > gap {
                    out.push((s0, last + 1));
                    start = None;
                }
            }
        }
        if let Some(s0) = start {
            out.push((s0, last + 1));
        }
        out
    };
    let mut hor = vec![false; w * h];
    let mut ver = vec![false; w * h];
    for y in 0..h {
        for (s0, e) in runs(w, gap, &|x| near_h[y * w + x]) {
            if e - s0 >= min_run {
                hor[y * w + s0..y * w + e].iter_mut().for_each(|v| *v = true);
            }
        }
    }
    for x in 0..w {
        for (s0, e) in runs(h, gap, &|y| near_v[y * w + x]) {
            if e - s0 >= min_run {
                for yy in s0..e {
                    ver[yy * w + x] = true;
                }
            }
        }
    }
    let hor: Vec<bool> = hor.iter().zip(&on).map(|(a, b)| *a && *b).collect();
    let ver: Vec<bool> = ver.iter().zip(&on).map(|(a, b)| *a && *b).collect();
    (hor, ver)
}

/// `remove_rules`, keeping the pixels of a printed line that have the ink's colour: a pen
/// stroke drawn right on a (blue, cyan, red) line stays.
fn remove_lines(alpha: &mut Gray, ppm: f32, rgb: Option<&Rgb>) {
    let (w, h) = (alpha.w, alpha.h);
    let (mut hor, mut ver) = line_masks(alpha, ppm);
    if let Some(rgb) = rgb {
        // what the lines alone look like: their pixels away from any dark (ink) pixel
        let dark: Vec<bool> = alpha.d.iter().map(|&v| v > 0.8).collect();
        let near_ink = img::dilate_mask(&dark, w, h, ((0.4 * ppm).round() as usize).max(2));
        let clean: Vec<usize> = (0..w * h).filter(|&i| (hor[i] || ver[i]) && alpha.d[i] > 0.1 && !near_ink[i]).collect();
        let chroma = |i: usize| -> [f32; 2] {
            let (r, g, b) = (rgb.d[i * 3], rgb.d[i * 3 + 1], rgb.d[i * 3 + 2]);
            let s = r + g + b + 1e-3;
            [r / s, g / s]
        };
        let med = |idx: &[usize]| -> Option<[f32; 2]> {
            if idx.len() < 50 {
                return None;
            }
            let step = (idx.len() / 20_000).max(1);
            let cs: Vec<[f32; 2]> = idx.iter().step_by(step).map(|&i| chroma(i)).collect();
            Some([0, 1].map(|k| signal::median(&cs.iter().map(|c| c[k]).collect::<Vec<_>>())))
        };
        let ink: Vec<usize> = (0..w * h).filter(|&i| !hor[i] && !ver[i] && alpha.d[i] > 0.6).collect();
        if let (Some(lc), Some(ic)) = (med(&clean), med(&ink)) {
            let d = |a: [f32; 2], b: [f32; 2]| (a[0] - b[0]).hypot(a[1] - b[1]);
            if d(lc, ic) > 0.03 {
                for i in 0..w * h {
                    if (hor[i] || ver[i]) && alpha.d[i] > 0.3 && d(chroma(i), ic) < d(chroma(i), lc) {
                        hor[i] = false;
                        ver[i] = false;
                    }
                }
            }
        }
    }
    let hor = img::dilate_mask(&hor, w, h, 1);
    let ver = img::dilate_mask(&ver, w, h, 1);
    if !hor.iter().any(|&b| b) && !ver.iter().any(|&b| b) {
        return;
    }
    let src = alpha.d.clone();
    // Horizontal rules: fill each masked run in a column from the pixels just above and below.
    for x in 0..w {
        let mut y = 0;
        while y < h {
            if hor[y * w + x] {
                let s = y;
                while y < h && hor[y * w + x] {
                    y += 1;
                }
                let a = if s > 0 { src[(s - 1) * w + x] } else { 0.0 };
                let b = if y < h { src[y * w + x] } else { 0.0 };
                let n = (y - s + 1) as f32;
                for (k, yy) in (s..y).enumerate() {
                    let t = (k + 1) as f32 / n;
                    // only keep ink if both sides have ink: a stroke crossing the line
                    let v = if a > 0.2 && b > 0.2 { a * (1.0 - t) + b * t } else { 0.0 };
                    alpha.d[yy * w + x] = v;
                }
            } else {
                y += 1;
            }
        }
    }
    let src = alpha.d.clone();
    for y in 0..h {
        let mut x = 0;
        while x < w {
            if ver[y * w + x] {
                let s = x;
                while x < w && ver[y * w + x] {
                    x += 1;
                }
                let a = if s > 0 { src[y * w + s - 1] } else { 0.0 };
                let b = if x < w { src[y * w + x] } else { 0.0 };
                let n = (x - s + 1) as f32;
                for (k, xx) in (s..x).enumerate() {
                    let t = (k + 1) as f32 / n;
                    alpha.d[y * w + xx] = if a > 0.2 && b > 0.2 { a * (1.0 - t) + b * t } else { 0.0 };
                }
            } else {
                x += 1;
            }
        }
    }
}

/// Remove specks smaller than `min_mm2`, and faint haze far from real strokes.
pub fn drop_specks(alpha: &Gray, ppm: f32, min_mm2: f32) -> Gray {
    let (w, h) = (alpha.w, alpha.h);
    let m: Vec<bool> = alpha.d.iter().map(|&v| v > 0.3).collect();
    let (lab, comps) = img::components(&m, w, h);
    let min_area = ((min_mm2 * ppm * ppm) as usize).max(4);
    let big: Vec<bool> = lab.iter().map(|&l| l != 0 && comps[l as usize].area >= min_area).collect();
    let support = img::dilate_mask(&big, w, h, 2);
    Gray { w, h, d: alpha.d.iter().zip(&support).map(|(&v, &s)| if s { v } else { 0.0 }).collect() }
}

/// (alpha in [0,1], ink reflectance rgb relative to paper).
/// `exclude`: pixel rects [x0, y0, x1, y1] that are printed, not handwritten (markers, headers).
pub fn ink_matte(rgb: &Rgb, ppm: f32, mode: Mode, exclude: &[[usize; 4]]) -> (Gray, [f32; 3]) {
    let ch = if mode == Mode::Sheet { rgb.channel(0) } else { rgb.min_channel() };
    let bg = background(&ch, ppm);
    let mut d: Vec<f32> = ch.d.par_iter().zip(&bg.d).map(|(&c, &b)| 1.0 - (c / b).clamp(0.0, 1.2)).collect();
    for r in exclude {
        for y in r[1]..r[3].min(rgb.h) {
            for x in r[0]..r[2].min(rgb.w) {
                d[y * rgb.w + x] = 0.0;
            }
        }
    }
    // noise floor from the brighter 60%
    let p60 = signal::percentile(&sample(&d, 200_000), 60.0);
    let paper: Vec<f32> = sample(&d, 200_000).into_iter().filter(|&v| v < p60).collect();
    let sigma = if paper.is_empty() { 0.02 } else { signal::mad_sigma(&paper) };
    let t0 = (4.0 * sigma).max(0.10);
    let strong_mask: Vec<bool> = d.iter().map(|&v| v > t0).collect();
    let hw = handwriting(&strong_mask, rgb.w, rgb.h, ppm);
    let strong: Vec<f32> = d.iter().zip(&hw).filter(|(v, h)| **h && **v > t0).map(|(v, _)| *v).collect();
    let all_strong = strong_mask.iter().filter(|&&m| m).count();
    // a printed grid joins every stroke into one sheet-sized shape, taken for a printed block:
    // then most dark pixels were left out, and the full-ink level is the darkest of the page
    let k = if strong.len() * 5 < all_strong {
        signal::percentile(&sample(&d, 400_000), 99.5).max(t0 + 0.15)
    } else if strong.len() > 50 {
        signal::percentile(&sample(&strong, 200_000), 90.0).max(t0 + 0.15)
    } else {
        0.5f32.max(t0 + 0.15)
    };
    let mut alpha = Gray { w: rgb.w, h: rgb.h, d: d.par_iter().map(|&v| ((v - t0) / (k - t0)).clamp(0.0, 1.0)).collect() };
    if mode == Mode::Generic {
        remove_lines(&mut alpha, ppm, Some(rgb));
    }
    let alpha = drop_specks(&alpha, ppm, 0.04);
    // Ink colour = colour where alpha reaches 1, so alpha x colour reproduces the photo.
    let mut acc = [Vec::new(), Vec::new(), Vec::new()];
    // paper colour per channel at low resolution, sampled only where the ink is solid
    let bgs: Vec<(Gray, usize)> = (0..3).map(|c| background_small(&rgb.channel(c), ppm)).collect();
    for i in 0..alpha.d.len() {
        if alpha.d[i] >= 0.98 && hw[i] {
            let (x, y) = ((i % rgb.w) as f32, (i / rgb.w) as f32);
            for c in 0..3 {
                let (g, f) = &bgs[c];
                let b = g.sample((x + 0.5) / *f as f32 - 0.5, (y + 0.5) / *f as f32 - 0.5).max(1e-3);
                acc[c].push(rgb.d[i * 3 + c] / b);
            }
        }
    }
    let ink = if acc[0].len() > 20 {
        [0, 1, 2].map(|c| signal::median(&acc[c]).clamp(0.02, 0.9))
    } else {
        [0.15, 0.17, 0.35]
    };
    (alpha, ink)
}

fn sample(v: &[f32], n: usize) -> Vec<f32> {
    if v.len() <= n {
        return v.to_vec();
    }
    let step = v.len() as f64 / n as f64;
    (0..n).map(|i| v[(i as f64 * step) as usize]).collect()
}

/// Tight crop around ink (> 0.2) with padding. Returns (crop, x0, y0).
pub fn tight_crop(a: &Gray, pad: usize) -> Option<(Gray, usize, usize)> {
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
    for y in 0..a.h {
        for x in 0..a.w {
            if a.d[y * a.w + x] > 0.2 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    if x0 == usize::MAX {
        return None;
    }
    let (x0, y0) = (x0.saturating_sub(pad), y0.saturating_sub(pad));
    let (x1, y1) = ((x1 + pad + 1).min(a.w), (y1 + pad + 1).min(a.h));
    Some((a.crop(x0, y0, x1, y1), x0, y0))
}
