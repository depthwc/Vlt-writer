//! Paper library entries: real photos of blank paper, measured once (sheet corners, ruled lines,
//! margin, noise, blur). Writing positions come from the detected lines, so text follows the
//! printed lines even where the sheet bends. A paper can hold several photos ("shots").
//!
//! The corners give the perspective of a flat sheet. A real page is rarely flat: it bulges
//! towards the camera near a notebook's spine, or lies tilted on a knee. The printed lines
//! show it, being farther apart where the page is nearer: letters are sized by that local
//! spacing (and by the squares' width on squared paper), and blurred as much as the photo is
//! blurred there, so writing nearer the camera comes out bigger and as sharp as the paper.

use serde::{Deserialize, Serialize};

use crate::geom::{self, Homography, P2};
use crate::img::{Gray, Rgb};
use crate::matte;
use crate::signal;

pub const ANALYSIS_PPM: f64 = 8.0;
pub const MESH: (usize, usize) = (7, 5);

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PaperKind {
    Lined,
    Grid,
    Blank,
}

/// Which page of a notebook a paper is. A document written in a notebook takes left and right
/// pages in turn (left, right, left…); any other paper, such as a loose white sheet, is used
/// as it is.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Left,
    Right,
    #[default]
    Any,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Shot {
    pub file: String,
    pub w: u32,
    pub h: u32,
    pub corners: [P2; 4],
    pub found: bool,
    /// horizontal printed lines, polylines in sheet mm
    pub rules: Vec<Vec<P2>>,
    #[serde(default)]
    pub rules_edited: bool,
    pub vlines: Vec<f64>,
    pub margin_mm: Option<f64>,
    pub noise: f32,
    pub blur_px: f32,
    pub ppm_photo: f64,
    /// blur over the sheet: (photo x, photo y, sigma px); parts out of the focus are blurrier
    #[serde(default)]
    pub blur_map: Vec<[f64; 3]>,
    /// the writing area's left and right edges bent: x shifts (mm) at BEND_N heights evenly down
    /// the sheet (a photo at an angle tilts and bends what is straight on the paper)
    #[serde(default)]
    pub area_bend: Option<[Vec<f64>; 2]>,
}

/// Which of `n` photos each of `pages` pages is written on: picked at random (from `seed`),
/// never the same photo twice running when there is another.
pub fn pick_photos(n: usize, pages: usize, seed: u64) -> Vec<usize> {
    use rand::{Rng, SeedableRng};
    let mut rng = rand_pcg::Pcg64Mcg::seed_from_u64(seed ^ 0x51ee_7a9e);
    let mut out: Vec<usize> = Vec::with_capacity(pages);
    for _ in 0..pages {
        let mut i = rng.random_range(0..n.max(1));
        if n > 1 && out.last() == Some(&i) {
            i = (i + rng.random_range(1..n)) % n;
        }
        out.push(i);
    }
    out
}

/// Heights down the sheet where the writing area's edges can bend.
pub const BEND_N: usize = 7;

/// x shift of a bent edge at sheet height `y` (mm), `h` the sheet's height.
pub fn bend_at(offs: &[f64], y: f64, h: f64) -> f64 {
    if offs.len() < 2 {
        return 0.0;
    }
    let ys: Vec<f64> = (0..offs.len()).map(|i| i as f64 * h / (offs.len() - 1) as f64).collect();
    signal::interp(y, &ys, offs)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PaperMeta {
    pub id: String,
    pub name: String,
    pub created: f64,
    pub size: String,
    pub kind: PaperKind,
    /// grid lines per written line
    pub step: usize,
    pub pitch_rule: f64,
    pub start_line: usize,
    pub left_mm: f64,
    pub right_mm: f64,
    pub shots: Vec<Shot>,
    #[serde(default)]
    pub side: Side,
}

/// Measure one photo. `corners`: user-adjusted corners, or None to detect.
pub fn measure_shot(photo: &Rgb, file: &str, corners: Option<[P2; 4]>, size: Option<&str>) -> (Shot, String) {
    let (corners, found) = match corners {
        Some(c) => (geom::order_corners(&c), true),
        None => geom::detect_sheet(photo),
    };
    let size_name: String = match size {
        Some(s) => s.to_string(),
        // a close-up of squared paper: its 5 mm squares give the real size of what is in view
        None if !found => match geom::grid_pitch(photo) {
            Some(p) => format!("{:.1}x{:.1}", photo.w as f64 * 5.0 / p, photo.h as f64 * 5.0 / p),
            None => geom::guess_size(&corners).to_string(),
        },
        None => geom::guess_size(&corners).to_string(),
    };
    let sz = geom::paper_size(&size_name);
    let h = geom::mm_to_px(&corners, sz);
    let flat = geom::rectify(photo, &h, sz, ANALYSIS_PPM);
    let (rules, vlines, margin) = detect_lines(&flat, ANALYSIS_PPM);
    // a printed margin is the writing area's edge on its side, as tilted as the photo shows it
    let left_side = |m: &[f64; 2]| (m[0] + m[1]) / 2.0 < sz.0 / 2.0;
    let margin_mm = margin.map(|m| if left_side(&m) { m[0].max(m[1]) } else { m[0].min(m[1]) });
    let area_bend = margin.zip(margin_mm).map(|(m, x)| {
        let follow: Vec<f64> = (0..BEND_N).map(|i| m[0] + (m[1] - m[0]) * i as f64 / (BEND_N - 1) as f64 - x).collect();
        if left_side(&m) { [follow, vec![0.0; BEND_N]] } else { [vec![0.0; BEND_N], follow] }
    });
    let c = h.apply([sz.0 / 2.0, sz.1 / 2.0]);
    let c1 = h.apply([sz.0 / 2.0 + 1.0, sz.1 / 2.0]);
    let ppm_photo = ((c1[0] - c[0]).powi(2) + (c1[1] - c[1]).powi(2)).sqrt();
    let g = photo.mean_channel();
    let shot = Shot {
        file: file.into(),
        w: photo.w as u32,
        h: photo.h as u32,
        corners,
        found,
        noise: noise(photo, &corners),
        blur_px: blur(&g, &h, &rules, ppm_photo),
        blur_map: blur_map(&g, &h, &rules, ppm_photo),
        rules,
        rules_edited: false,
        vlines,
        margin_mm,
        ppm_photo,
        area_bend,
    };
    (shot, size_name)
}

impl PaperMeta {
    pub fn new(id: &str, name: &str, first: Shot, size: &str) -> Self {
        let mut m = PaperMeta {
            id: id.into(),
            name: name.into(),
            created: crate::hand::now(),
            size: size.into(),
            kind: PaperKind::Blank,
            step: 1,
            pitch_rule: 0.0,
            start_line: 0,
            left_mm: 12.0,
            right_mm: geom::paper_size(size).0 - 8.0,
            shots: vec![first],
            side: Side::Any,
        };
        m.classify();
        m.defaults();
        m
    }

    /// Lined / grid / blank from the main shot.
    pub fn classify(&mut self) {
        let s = &self.shots[0];
        let ys: Vec<f32> = s.rules.iter().map(|l| (l.iter().map(|p| p[1]).sum::<f64>() / l.len() as f64) as f32).collect();
        let pitch = if ys.len() > 2 { signal::median(&ys.windows(2).map(|w| w[1] - w[0]).collect::<Vec<_>>()) as f64 } else { 0.0 };
        let vp = if s.vlines.len() > 2 { signal::median(&s.vlines.windows(2).map(|w| (w[1] - w[0]) as f32).collect::<Vec<_>>()) as f64 } else { 0.0 };
        let grid = s.vlines.len() >= 6 && pitch > 0.0 && (vp - pitch).abs() < 0.25 * pitch;
        self.pitch_rule = pitch;
        let kind = if grid { PaperKind::Grid } else if s.rules.len() >= 5 { PaperKind::Lined } else { PaperKind::Blank };
        if kind != self.kind {
            self.step = if kind == PaperKind::Grid { 2 } else { 1 };
        }
        self.kind = kind;
    }

    pub fn defaults(&mut self) {
        let (w, _) = geom::paper_size(&self.size);
        let lines = self.writing_lines(0);
        self.start_line = lines.iter().position(|l| l.iter().map(|p| p[1]).sum::<f64>() / l.len() as f64 >= 12.0).unwrap_or(0);
        // the printed margin: on a left page at the left, on a right page at the right
        let margin = self.shots[0].margin_mm;
        self.left_mm = match margin {
            Some(m) if m < w / 2.0 => m + 3.0,
            _ => 12.0,
        };
        self.right_mm = match margin {
            Some(m) if m > w / 2.0 => m - 3.0,
            _ => w - 8.0,
        };
    }

    /// Baselines to write on for one shot, sheet mm polylines.
    pub fn writing_lines(&self, shot: usize) -> Vec<Vec<P2>> {
        let s = &self.shots[shot.min(self.shots.len() - 1)];
        let (w, h) = geom::paper_size(&self.size);
        // printed lines, or lines the person drew on blank paper
        if !s.rules.is_empty() && (self.kind != PaperKind::Blank || s.rules_edited) {
            return s.rules.iter().step_by(self.step.max(1)).cloned().collect();
        }
        let xs: Vec<f64> = (0..9).map(|i| i as f64 * w / 8.0).collect();
        let mut out = vec![];
        let mut y = 22.0;
        while y < h - 12.0 {
            out.push(xs.iter().map(|&x| [x, y]).collect());
            y += 9.0;
        }
        out
    }
}

// --------------------------------------------------------------------------- line detection
fn track(profiles: &[Vec<f32>], centers: &[f64], ppm: f64) -> Vec<Vec<P2>> {
    let n = profiles[0].len();
    let full: Vec<f32> = (0..n).map(|i| profiles.iter().map(|p| p[i]).sum::<f32>() / profiles.len() as f32).collect();
    let m = signal::mean(&full);
    let z: Vec<f32> = full.iter().map(|v| v - m).collect();
    let ac = |lag: usize| -> f32 { (0..n - lag).map(|i| z[i] * z[i + lag]).sum() };
    let (lo, hi) = ((3.0 * ppm) as usize, (16.0 * ppm) as usize);
    let a0 = ac(0);
    if hi >= n || a0 <= 0.0 {
        return vec![];
    }
    let pitch = (lo..hi).max_by(|&a, &b| ac(a).partial_cmp(&ac(b)).unwrap()).unwrap();
    if ac(pitch) < 0.15 * a0 {
        return vec![];
    }
    let peaks: Vec<Vec<usize>> = profiles
        .iter()
        .map(|p| {
            let noise = signal::mad_sigma(p) + 1e-4;
            signal::find_peaks(p, ((0.6 * pitch as f64) as usize).max(2), (4.0 * noise).max(0.01)).into_iter().map(|q| q.0).collect()
        })
        .collect();
    let rf = (0..peaks.len()).max_by_key(|&i| peaks[i].len()).unwrap();
    let mut lines = vec![];
    for &y0 in &peaks[rf] {
        let mut ys: Vec<(usize, f64)> = vec![(rf, y0 as f64)];
        for dir in [1isize, -1] {
            let (mut y, mut i) = (y0 as f64, rf as isize + dir);
            while i >= 0 && (i as usize) < peaks.len() {
                if let Some(&q) = peaks[i as usize].iter().min_by(|a, b| (**a as f64 - y).abs().partial_cmp(&(**b as f64 - y).abs()).unwrap()) {
                    if (q as f64 - y).abs() < 0.3 * pitch as f64 {
                        y = q as f64;
                        ys.push((i as usize, y));
                    }
                }
                i += dir;
            }
        }
        if ys.len() as f64 >= 0.5 * peaks.len() as f64 {
            ys.sort_by_key(|v| v.0);
            lines.push(ys.iter().map(|&(i, y)| [centers[i], y]).collect());
        }
    }
    lines
}

/// Evenly spaced printed lines (a squared or lined page) running across `d`: in narrow bands the
/// darkness peaks are fitted as a comb (start, spacing, and a little change of spacing down the
/// page, as the page bends away from the camera); start, spacing and change are then smoothed
/// across the sheet. Lines at `xs` (px); y in px. None when there is no such comb.
fn comb_lines(d: &Gray, pitch: f64, xs: &[f64]) -> Option<Vec<Vec<P2>>> {
    let (w, h) = (d.w, d.h);
    let width = ((2.0 * pitch) as usize).clamp(8, w);
    let step = ((pitch / 2.0) as usize).max(4);
    let starts: Vec<usize> = (0..).map(|k| k * step).take_while(|&b| b + width <= w).collect();
    if starts.len() < 4 {
        return None;
    }
    let peaks: Vec<Vec<f64>> = starts
        .iter()
        .map(|&b0| {
            let prof: Vec<f32> = (0..h).map(|y| signal::median(&d.d[y * w + b0..y * w + b0 + width])).collect();
            let base = signal::gauss1d(&prof, pitch as f32);
            let z: Vec<f32> = signal::gauss1d(&prof, 1.0).iter().zip(&base).map(|(a, b)| a - b).collect();
            let noise = signal::mad_sigma(&z) + 1e-4;
            signal::find_peaks(&z, ((0.7 * pitch) as usize).max(2), (3.0 * noise).max(0.004)).into_iter().map(|p| p.0 as f64).collect()
        })
        .collect();
    let mid = starts.len() / 2;
    let mut order: Vec<usize> = (0..starts.len()).collect();
    order.sort_by_key(|&k| (k as isize - mid as isize).unsigned_abs());
    let mut fit: Vec<Option<[f64; 3]>> = vec![None; starts.len()];
    for &k in &order {
        let pk = &peaks[k];
        if pk.len() < 6 {
            continue;
        }
        // line 0: the one nearest the middle, or the neighbouring band's line 0
        let near = [k.checked_sub(1), Some(k + 1)].into_iter().flatten().find_map(|j| fit.get(j).copied().flatten());
        let a0 = match near {
            Some(f) => f[0],
            None => *pk.iter().min_by(|a, b| (*a - h as f64 / 2.0).abs().partial_cmp(&(*b - h as f64 / 2.0).abs()).unwrap()).unwrap(),
        };
        let mut sol = [a0, near.map_or(pitch, |f| f[1]), 0.0];
        let mut ok = true;
        for it in 0..5 {
            let tol = if it == 0 { 0.35 } else { 0.15 } * sol[1];
            let pred = |i: f64, s: &[f64; 3]| s[0] + s[1] * i + s[2] * i * i;
            let inl: Vec<(f64, f64)> = pk.iter().map(|&y| (((y - sol[0]) / sol[1]).round(), y)).filter(|&(i, y)| (pred(i, &sol) - y).abs() < tol).collect();
            if inl.len() < 6 {
                ok = false;
                break;
            }
            let c = signal::polyfit(&inl.iter().map(|v| v.0).collect::<Vec<_>>(), &inl.iter().map(|v| v.1).collect::<Vec<_>>(), 2);
            sol = [c[2], c[1], c[0]];
        }
        if ok && sol[1] > 0.6 * pitch && sol[1] < 1.5 * pitch {
            fit[k] = Some(sol);
        }
    }
    let good: Vec<(f64, [f64; 3])> = starts.iter().zip(&fit).filter_map(|(&b0, f)| f.map(|f| (b0 as f64 + width as f64 / 2.0, f))).collect();
    if good.len() < 4 || (good.len() as f64) < 0.5 * starts.len() as f64 {
        return None;
    }
    // start, spacing and change across the sheet, smooth (one band's slip of a whole line dropped)
    let cs: Vec<f64> = good.iter().map(|g| g.0).collect();
    let smooth = |k: usize, tol: f64| -> Vec<f64> {
        let v: Vec<f64> = good.iter().map(|g| g.1[k]).collect();
        let deg = if cs.len() >= 6 { 2 } else { 1 };
        let c = signal::polyfit(&cs, &v, deg);
        let keep: Vec<usize> = (0..v.len()).filter(|&i| (signal::polyval(&c, cs[i]) - v[i]).abs() < tol).collect();
        if keep.len() >= 4 && keep.len() < v.len() {
            signal::polyfit(&keep.iter().map(|&i| cs[i]).collect::<Vec<_>>(), &keep.iter().map(|&i| v[i]).collect::<Vec<_>>(), deg)
        } else {
            c
        }
    };
    let (fa, fp, fc) = (smooth(0, 0.3 * pitch), smooth(1, 0.1 * pitch), smooth(2, 0.02 * pitch));
    let pred = |i: f64, x: f64| signal::polyval(&fa, x) + signal::polyval(&fp, x) * i + signal::polyval(&fc, x) * i * i;
    let n = (h as f64 / pitch).ceil() as i64 + 3;
    // Where the page curls the comb is off, by up to half a line: each line is followed from the
    // middle band outwards, band by band, staying on the peak nearest its last offset.
    let ids: Vec<i64> = (-n..=n).collect();
    let cx: Vec<f64> = starts.iter().map(|&b| b as f64 + width as f64 / 2.0).collect();
    let k0 = *order.iter().find(|&&k| fit[k].is_some()).unwrap();
    let nearest = |k: usize, y: f64| peaks[k].iter().copied().min_by(|a, b| (a - y).abs().partial_cmp(&(b - y).abs()).unwrap());
    let shift: Vec<Vec<Option<f64>>> = ids
        .iter()
        .map(|&i| {
            let i = i as f64;
            let mut out = vec![None; starts.len()];
            let r0 = nearest(k0, pred(i, cx[k0])).map(|q| q - pred(i, cx[k0])).filter(|r| r.abs() < 0.3 * pitch);
            out[k0] = r0;
            for dir in [1isize, -1] {
                let mut r = r0.unwrap_or(0.0);
                let mut k = k0 as isize + dir;
                while k >= 0 && (k as usize) < starts.len() {
                    let ku = k as usize;
                    let yp = pred(i, cx[ku]);
                    if let Some(q) = nearest(ku, yp + r).filter(|q| (q - yp - r).abs() < 0.2 * pitch) {
                        r = q - yp;
                        out[ku] = Some(r);
                    }
                    k += dir;
                }
            }
            out
        })
        .collect();
    // neighbouring lines bend alike: a line that slipped onto the next one takes their offset
    let mut shift = shift;
    for _ in 0..2 {
        let prev = shift.clone();
        for li in 0..prev.len() {
            for k in 0..starts.len() {
                let v: Vec<f32> = (li.saturating_sub(2)..(li + 3).min(prev.len())).filter(|&l| l != li).filter_map(|l| prev[l][k].map(|s| s as f32)).collect();
                if v.len() >= 2 {
                    let m = signal::median(&v) as f64;
                    if prev[li][k].is_none_or(|s| (s - m).abs() > 0.25 * pitch) {
                        shift[li][k] = Some(m);
                    }
                }
            }
        }
    }
    let smoothed: Vec<Vec<Option<f64>>> = shift
        .iter()
        .map(|row| {
            (0..row.len())
                .map(|k| {
                    let v: Vec<f32> = row[k.saturating_sub(2)..(k + 3).min(row.len())].iter().flatten().map(|&s| s as f32).collect();
                    (v.len() >= 2).then(|| signal::median(&v) as f64)
                })
                .collect()
        })
        .collect();
    // then averaged: a slip of part of a line becomes a gentle slope rather than a step
    let smoothed: Vec<Vec<Option<f64>>> = smoothed
        .iter()
        .map(|row| {
            (0..row.len())
                .map(|k| {
                    let v: Vec<f64> = row[k.saturating_sub(3)..(k + 4).min(row.len())].iter().flatten().copied().collect();
                    row[k].map(|_| v.iter().sum::<f64>() / v.len() as f64)
                })
                .collect()
        })
        .collect();
    let at = |li: usize, x: f64| -> f64 {
        let s: Vec<(f64, f64)> = cx.iter().zip(&smoothed[li]).filter_map(|(&c, v)| v.map(|v| (c, v))).collect();
        match s.iter().position(|p| p.0 >= x) {
            None => s.last().map_or(0.0, |p| p.1),
            Some(0) => s[0].1,
            Some(j) => {
                let (a, b) = (s[j - 1], s[j]);
                a.1 + (b.1 - a.1) * (x - a.0) / (b.0 - a.0)
            }
        }
    };
    let mut lines = vec![];
    for (li, &i) in ids.iter().enumerate() {
        let line: Vec<P2> = xs.iter().map(|&x| [x, pred(i as f64, x) + at(li, x)]).collect();
        let inside = line.iter().filter(|p| p[1] >= 0.0 && p[1] < h as f64).count();
        if inside * 10 >= line.len() * 8 {
            lines.push(line);
        }
    }
    (lines.len() >= 6).then_some(lines)
}

/// Lines evenly spaced (gaps within a quarter of the usual one)?
fn regular(rules: &[Vec<P2>], pitch_mm: f64) -> bool {
    if rules.len() < 4 {
        return false;
    }
    let gaps: Vec<f64> = rules.windows(2).map(|w| w[1][5][1] - w[0][5][1]).collect();
    gaps.iter().filter(|&&g| (g - pitch_mm).abs() < 0.25 * pitch_mm).count() * 100 >= gaps.len() * 85
}

/// Points kept per detected line (a bent page needs them close together).
const LINE_POINTS: usize = 21;

/// The printed lines themselves, followed finely: every 2 mm across the sheet a 2 mm wide strip
/// gives the rows darker than around them (ridges); from a few columns each ridge is followed
/// outward, heading the way its own last steps went, onto the nearest ridge each time. Where the
/// photo bends the page this keeps every line on its printed line, which a fit of the whole
/// sheet cannot. `rules`: the lines found coarsely (mm), kept when too few ridges are followed;
/// `d`: darkness at `ppm` px/mm.
fn refine_lines(d: &Gray, rules: &[Vec<P2>], ppm: f64) -> Vec<Vec<P2>> {
    let (w, h) = (d.w, d.h);
    let mids: Vec<f32> = rules.iter().map(|l| l[l.len() / 2][1] as f32).collect();
    if rules.len() < 4 || w < (8.0 * ppm) as usize {
        return rules.to_vec();
    }
    let pitch = signal::median(&mids.windows(2).map(|g| g[1] - g[0]).collect::<Vec<_>>()) as f64 * ppm;
    let half = (ppm as usize).max(2);
    let xs: Vec<usize> = (0..).map(|k| half + k * 2 * half).take_while(|&x| x + half < w).collect();
    let n = xs.len();
    if n < 8 || pitch < 4.0 {
        return rules.to_vec();
    }
    // per station: the strip's darkness down the sheet, and its ridges (sub-pixel rows)
    let strips: Vec<Vec<f32>> = xs.iter().map(|&x| (0..h).map(|y| d.d[y * w + x - half..y * w + x + half].iter().sum::<f32>() / (2 * half) as f32).collect()).collect();
    let ridges: Vec<Vec<f64>> = strips
        .iter()
        .map(|p| {
            let base = signal::gauss1d(p, pitch as f32);
            let z: Vec<f32> = p.iter().zip(&base).map(|(a, b)| a - b).collect();
            let noise = signal::mad_sigma(&z) + 1e-4;
            signal::find_peaks(&z, ((0.6 * pitch) as usize).max(2), (3.0 * noise).max(0.006))
                .into_iter()
                // a thin dark row with paper on both sides, not the edge of the sheet against the desk
                .filter(|&(i, _)| {
                    let r = ((pitch / 5.0) as usize).max(3);
                    let mean = |v: &[f32]| v.iter().sum::<f32>() / v.len().max(1) as f32;
                    i >= r + 2 && i + r + 2 < h && mean(&p[i - 2 - r..i - 2]) < 0.07 && mean(&p[i + 3..i + 3 + r]) < 0.07
                })
                .map(|(i, _)| {
                    let (a, b, c) = (z[i - 1] as f64, z[i] as f64, z[i + 1] as f64);
                    let den = a - 2.0 * b + c;
                    i as f64 + if den.abs() > 1e-9 { (0.5 * (a - c) / den).clamp(-0.5, 0.5) } else { 0.0 }
                })
                .collect()
        })
        .collect();
    let nearest = |k: usize, y: f64, r: f64| ridges[k].iter().copied().filter(|q| (q - y).abs() < r).min_by(|a, b| (a - y).abs().partial_cmp(&(b - y).abs()).unwrap());
    // the coarse lines at the stations: a first heading for a ridge
    let was: Vec<Vec<f64>> = rules
        .iter()
        .map(|l| {
            let (lx, ly): (Vec<f64>, Vec<f64>) = l.iter().map(|q| (q[0] * ppm, q[1] * ppm)).unzip();
            xs.iter().map(|&x| signal::interp(x as f64, &lx, &ly)).collect()
        })
        .collect();
    let mut chains: Vec<Vec<Option<f64>>> = vec![];
    for ks in [n / 2, n / 4, 3 * n / 4] {
        for &q in &ridges[ks] {
            if chains.iter().any(|c| c[ks].is_some_and(|y| (y - q).abs() < 0.4 * pitch)) {
                continue;
            }
            let guide = was.iter().min_by(|a, b| (a[ks] - q).abs().partial_cmp(&(b[ks] - q).abs()).unwrap()).unwrap();
            let mut c = vec![None; n];
            c[ks] = Some(q);
            for dir in [1isize, -1] {
                let mut seen: Vec<(f64, f64)> = vec![(ks as f64, q)];
                let mut k = ks as isize + dir;
                while k >= 0 && (k as usize) < n {
                    let ku = k as usize;
                    let (kl, yl) = *seen.last().unwrap();
                    let recent: Vec<(f64, f64)> = seen.iter().rev().take_while(|p| (p.0 - ku as f64).abs() <= 6.0).copied().collect();
                    let slope = if recent.len() >= 3 {
                        signal::polyfit(&recent.iter().map(|p| p.0).collect::<Vec<_>>(), &recent.iter().map(|p| p.1).collect::<Vec<_>>(), 1)[0]
                    } else {
                        (guide[ku] - guide[kl as usize]) / (ku as f64 - kl)
                    };
                    if let Some(y) = nearest(ku, yl + slope * (ku as f64 - kl), 0.22 * pitch) {
                        seen.push((ku as f64, y));
                        c[ku] = Some(y);
                    }
                    k += dir;
                }
            }
            chains.push(c);
        }
    }
    // the same ridge followed twice: the shorter goes
    let len = |c: &Vec<Option<f64>>| c.iter().flatten().count();
    chains.sort_by_key(|c| std::cmp::Reverse(len(c)));
    let mut kept: Vec<Vec<Option<f64>>> = vec![];
    for c in chains {
        let dup = kept.iter().any(|o| {
            let both = (0..n).filter(|&k| c[k].is_some() && o[k].is_some()).count();
            let same = (0..n).filter(|&k| matches!((c[k], o[k]), (Some(a), Some(b)) if (a - b).abs() < 0.3 * pitch)).count();
            both > 0 && same * 2 > both.min(len(&c))
        });
        if !dup && len(&c) * 10 >= n * 4 {
            kept.push(c);
        }
    }
    // where two still meet, neither is trusted there
    for k in 0..n {
        let at: Vec<(usize, f64)> = kept.iter().enumerate().filter_map(|(i, c)| c[k].map(|y| (i, y))).collect();
        for a in 0..at.len() {
            for b in a + 1..at.len() {
                if (at[a].1 - at[b].1).abs() < 0.5 * pitch {
                    kept[at[a].0][k] = None;
                    kept[at[b].0][k] = None;
                }
            }
        }
    }
    // each line: smoothed along itself, held straight on past its ends
    let lines: Vec<Vec<P2>> = kept
        .iter()
        .filter_map(|c| {
            let pts: Vec<(f64, f64)> = (0..n)
                .filter(|&k| c[k].is_some())
                .map(|k| {
                    let v: Vec<f32> = c[k.saturating_sub(2)..(k + 3).min(n)].iter().flatten().map(|&y| y as f32).collect();
                    (xs[k] as f64, signal::median(&v) as f64)
                })
                .collect();
            if pts.len() * 10 < n * 4 {
                return None;
            }
            let (px, py): (Vec<f64>, Vec<f64>) = pts.into_iter().unzip();
            let m = px.len();
            let end = |r: std::ops::Range<usize>| signal::polyfit(&px[r.clone()], &py[r], 1);
            let (head, tail) = (end(0..m.min(6)), end(m.saturating_sub(6)..m));
            let wmm = w as f64 / ppm;
            Some(
                (0..LINE_POINTS)
                    .map(|i| {
                        let x = i as f64 * wmm / (LINE_POINTS - 1) as f64;
                        let xp = x * ppm;
                        let y = if xp < px[0] {
                            signal::polyval(&head, xp)
                        } else if xp > px[m - 1] {
                            signal::polyval(&tail, xp)
                        } else {
                            signal::interp(xp, &px, &py)
                        };
                        [x, y / ppm]
                    })
                    .collect(),
            )
        })
        .collect();
    let mut lines: Vec<Vec<P2>> = lines.into_iter().filter(|l| l[LINE_POINTS / 2][1] > 1.5 && l[LINE_POINTS / 2][1] < h as f64 / ppm - 1.5).collect();
    lines.sort_by(|a, b| a[LINE_POINTS / 2][1].partial_cmp(&b[LINE_POINTS / 2][1]).unwrap());
    // a crease or a shadow between two printed lines is not one
    let p_mm = pitch / ppm;
    let mid = |l: &Vec<P2>| l[LINE_POINTS / 2][1];
    let odd: Vec<usize> = (1..lines.len().saturating_sub(1)).filter(|&j| mid(&lines[j]) - mid(&lines[j - 1]) < 0.65 * p_mm && mid(&lines[j + 1]) - mid(&lines[j]) < 0.65 * p_mm).collect();
    for &j in odd.iter().rev() {
        lines.remove(j);
    }
    if (lines.len() as f64) < 0.7 * rules.len() as f64 {
        return rules.to_vec();
    }
    lines
}

/// Horizontal ruled lines (polylines mm), vertical line positions (mm), red margin x (mm) at
/// the top and the bottom of the sheet.
pub fn detect_lines(flat: &Rgb, ppm: f64) -> (Vec<Vec<P2>>, Vec<f64>, Option<[f64; 2]>) {
    let ch = flat.min_channel();
    let bg = matte::background(&ch, ppm as f32);
    let d = Gray { w: ch.w, h: ch.h, d: ch.d.iter().zip(&bg.d).map(|(c, b)| (1.0 - c / b).clamp(0.0, 1.0)).collect() };
    let (w, h) = (d.w, d.h);
    let n = 14;
    let edges: Vec<usize> = (0..=n).map(|i| i * w / n).collect();
    // The sheet edge (desk showing through a slightly-off corner) must not dominate.
    let edge = (2.5 * ppm) as usize;
    let profiles: Vec<Vec<f32>> = (0..n)
        .map(|i| {
            let (a, b) = (edges[i], edges[i + 1]);
            let mut rows: Vec<f32> = (0..h).map(|y| d.d[y * w + a..y * w + b].iter().sum::<f32>() / (b - a).max(1) as f32).collect();
            let cap = signal::percentile(&rows, 99.0);
            for (y, v) in rows.iter_mut().enumerate() {
                if y < edge || y + edge >= h {
                    *v = 0.0;
                }
                *v = v.min(cap);
            }
            signal::gauss1d(&rows, 0.6)
        })
        .collect();
    let centers: Vec<f64> = (0..n).map(|i| (edges[i] + edges[i + 1]) as f64 / 2.0).collect();
    let xs_mm: Vec<f64> = (0..11).map(|i| i as f64 * (w as f64 / ppm) / 10.0).collect();
    let mut rules: Vec<Vec<P2>> = track(&profiles, &centers, ppm)
        .into_iter()
        .map(|t| {
            let x: Vec<f64> = t.iter().map(|p| p[0]).collect();
            let y: Vec<f64> = t.iter().map(|p| p[1]).collect();
            let deg = if x.len() >= 6 { 2 } else { 1 };
            let mut c = signal::polyfit(&x, &y, deg);
            let res: Vec<f32> = x.iter().zip(&y).map(|(&xv, &yv)| (signal::polyval(&c, xv) - yv).abs() as f32).collect();
            if res.iter().any(|&r| r > 1.5) && x.len() > deg + 2 {
                let p80 = signal::percentile(&res, 80.0);
                let keep: Vec<usize> = (0..x.len()).filter(|&i| res[i] <= p80).collect();
                c = signal::polyfit(&keep.iter().map(|&i| x[i]).collect::<Vec<_>>(), &keep.iter().map(|&i| y[i]).collect::<Vec<_>>(), deg);
            }
            xs_mm.iter().map(|&xm| [xm, signal::polyval(&c, xm * ppm) / ppm]).collect()
        })
        .collect();
    rules.sort_by(|a, b| a[5][1].partial_cmp(&b[5][1]).unwrap());
    // Drop edge artefacts: lines hugging the sheet border, and end lines off the regular pitch.
    let hmm = h as f64 / ppm;
    rules.retain(|l| l[5][1] > 1.5 && l[5][1] < hmm - 1.5);
    if rules.len() >= 4 {
        let gaps: Vec<f32> = rules.windows(2).map(|w| (w[1][5][1] - w[0][5][1]) as f32).collect();
        let pitch = signal::median(&gaps) as f64;
        while rules.len() >= 4 && ((rules[1][5][1] - rules[0][5][1]) - pitch).abs() > 0.35 * pitch {
            rules.remove(0);
        }
        while rules.len() >= 4 && ((rules[rules.len() - 1][5][1] - rules[rules.len() - 2][5][1]) - pitch).abs() > 0.35 * pitch {
            rules.pop();
        }
    }
    // vertical lines
    let cols: Vec<f32> = (0..w).map(|x| (0..h).map(|y| d.d[y * w + x]).sum::<f32>() / h as f32).collect();
    let vprof = signal::gauss1d(&cols, 0.6);
    let noise = signal::mad_sigma(&vprof) + 1e-4;
    let mut vpk: Vec<usize> = signal::find_peaks(&vprof, (3.0 * ppm) as usize, (4.0 * noise).max(0.01)).into_iter().map(|p| p.0).collect();
    // Evenly spaced lines found as a comb: on a page photographed close and a little bent the
    // lines drift and curve, and following them from band to band loses some
    let (lo, hi) = ((2.5 * ppm) as usize, (12.0 * ppm) as usize);
    // faint printed lines only: the desk or a shadow at the edge of the photo would swamp them
    let d = Gray { w, h, d: d.d.iter().map(|v| v.min(0.15)).collect() };
    if let Some(p) = geom::repeat_pitch(&d, true, lo, hi) {
        let xs_px: Vec<f64> = xs_mm.iter().map(|x| x * ppm).collect();
        if let Some(comb) = comb_lines(&d, p, &xs_px) {
            if comb.len() as f64 > 1.15 * rules.len() as f64 || !regular(&rules, p / ppm) {
                rules = comb.into_iter().map(|l| l.into_iter().map(|q| [q[0] / ppm, q[1] / ppm]).collect()).collect();
                rules.retain(|l: &Vec<P2>| l[5][1] > 1.5 && l[5][1] < hmm - 1.5);
            }
            // a squared page: the same comb down the page
            if let Some(pv) = geom::repeat_pitch(&d, false, lo, hi).filter(|pv| (pv - p).abs() < 0.2 * p) {
                let t = Gray { w: h, h: w, d: (0..w * h).map(|i| d.d[(i % h) * w + i / h]).collect() };
                if let Some(vc) = comb_lines(&t, pv, &[h as f64 / 2.0]) {
                    vpk = vc.iter().map(|l| l[0][1].round().max(0.0) as usize).filter(|&x| x < w).collect();
                }
            }
        }
    }
    // every line onto its printed line, finely (the bands above are a centimetre wide)
    let rules = refine_lines(&d, &rules, ppm);
    // red margin: a printed line clearly redder than the paper around it, found band by band down
    // the page and taken only when the bands line up (a photo at an angle tilts it; scattered
    // pink specks are not a margin: a wrong margin is worse than none)
    let nb = 8;
    let edge = (3.0 * ppm) as usize;
    let mut best: Vec<(f64, f64, f32, f32)> = vec![]; // band middle y, x, excess, next best excess
    for b in 0..nb {
        let (y0, y1) = (b * h / nb, (b + 1) * h / nb);
        let red: Vec<f32> = (0..w)
            .map(|x| (y0..y1).map(|y| {
                let i = (y * w + x) * 3;
                (flat.d[i] - flat.d[i + 1].max(flat.d[i + 2])).clamp(0.0, 1.0)
            }).sum::<f32>() / (y1 - y0).max(1) as f32)
            .collect();
        let red = signal::gauss1d(&red, 0.8);
        let around = signal::gauss1d(&red, (5.0 * ppm) as f32);
        let ex: Vec<f32> = red.iter().zip(&around).map(|(a, b)| a - b).collect();
        if w <= 2 * edge {
            break;
        }
        let x = (edge..w - edge).max_by(|&a, &b| ex[a].partial_cmp(&ex[b]).unwrap()).unwrap();
        let next = (edge..w - edge).filter(|&v| (v as f64 - x as f64).abs() > 3.0 * ppm).map(|v| ex[v]).fold(0.0f32, f32::max);
        best.push(((y0 + y1) as f64 / 2.0, x as f64, ex[x], next));
    }
    // the straight line through two bands that most others lie on
    let mut fit: Option<(f64, f64, Vec<usize>)> = None;
    for i in 0..best.len() {
        for j in i + 1..best.len() {
            let b = (best[j].1 - best[i].1) / (best[j].0 - best[i].0);
            let a = best[i].1 - b * best[i].0;
            let inl: Vec<usize> = (0..best.len()).filter(|&k| (a + b * best[k].0 - best[k].1).abs() < 1.5 * ppm).collect();
            if b.abs() < 0.1 && fit.as_ref().is_none_or(|f| inl.len() > f.2.len()) {
                fit = Some((a, b, inl));
            }
        }
    }
    let margin = fit.filter(|f| f.2.len() * 8 >= nb * 5).and_then(|(a, b, inl)| {
        let ex: Vec<f32> = inl.iter().map(|&k| best[k].2).collect();
        let next: Vec<f32> = inl.iter().map(|&k| best[k].3).collect();
        let clear = signal::median(&ex) >= 0.012 && signal::median(&ex) >= 1.8 * signal::median(&next);
        clear.then(|| {
            let (top, bottom) = (a, a + b * h as f64);
            vpk.retain(|&v| (v as f64 - (top + bottom) / 2.0).abs() > 1.5 * ppm);
            [top / ppm, bottom / ppm]
        })
    });
    (rules, vpk.iter().map(|&v| v as f64 / ppm).collect(), margin)
}

/// Immerkaer (1996) noise sigma: Laplacian-difference kernel, median for robustness.
fn noise(photo: &Rgb, corners: &[P2; 4]) -> f32 {
    let cx = (corners.iter().map(|c| c[0]).sum::<f64>() / 4.0) as usize;
    let cy = (corners.iter().map(|c| c[1]).sum::<f64>() / 4.0) as usize;
    let r = 400;
    let (x0, y0) = (cx.saturating_sub(r), cy.saturating_sub(r));
    let (x1, y1) = ((cx + r).min(photo.w), (cy + r).min(photo.h));
    if x1 < x0 + 3 || y1 < y0 + 3 {
        return 0.006;
    }
    let g = photo.mean_channel().crop(x0, y0, x1, y1);
    let k = [1.0, -2.0, 1.0, -2.0, 4.0, -2.0, 1.0, -2.0, 1.0];
    let mut v = Vec::with_capacity(g.w * g.h);
    for y in 1..g.h - 1 {
        for x in 1..g.w - 1 {
            let mut a = 0.0;
            for j in 0..3 {
                for i in 0..3 {
                    a += k[j * 3 + i] * g.at(x + i - 1, y + j - 1);
                }
            }
            v.push(a.abs());
        }
    }
    (signal::median(&v) / 0.6745 / 6.0).clamp(0.002, 0.05)
}

/// Gaussian blur (photo px) from the cross-section of a printed line near the centre.
fn blur(g: &Gray, h: &Homography, rules: &[Vec<P2>], ppm_photo: f64) -> f32 {
    if rules.is_empty() {
        return 0.8;
    }
    let l = &rules[rules.len() / 2];
    let (xa, xb) = (l[2][0], l[l.len() - 3][0]);
    line_blur(g, h, l, xa, xb, 25, ppm_photo).unwrap_or(0.8)
}

/// Blur at nine places of the sheet (three lines, three parts of each): a page nearer or
/// farther than the focus is blurrier there.
fn blur_map(g: &Gray, h: &Homography, rules: &[Vec<P2>], ppm_photo: f64) -> Vec<[f64; 3]> {
    let n = rules.len();
    if n < 6 {
        return vec![];
    }
    let mut out = vec![];
    for r in [n / 6, n / 2, 5 * n / 6] {
        let l = &rules[r];
        let (x0, x1) = (l[1][0], l[l.len() - 2][0]);
        for k in 0..3 {
            let (xa, xb) = (x0 + (x1 - x0) * k as f64 / 3.0, x0 + (x1 - x0) * (k + 1) as f64 / 3.0);
            if let Some(s) = line_blur(g, h, l, xa, xb, 9, ppm_photo) {
                let x = (xa + xb) / 2.0;
                let ly: Vec<f64> = l.iter().map(|p| p[1]).collect();
                let lx: Vec<f64> = l.iter().map(|p| p[0]).collect();
                let q = h.apply([x, signal::interp(x, &lx, &ly)]);
                out.push([q[0], q[1], s as f64]);
            }
        }
    }
    if out.len() < 6 {
        return vec![];
    }
    out
}

/// Blur (photo px) of one printed line between sheet x `xa` and `xb`, from `samples`
/// cross-sections; None when too few show the line.
fn line_blur(g: &Gray, h: &Homography, l: &[P2], xa: f64, xb: f64, samples: usize, ppm_photo: f64) -> Option<f32> {
    let lx: Vec<f64> = l.iter().map(|p| p[0]).collect();
    let ly: Vec<f64> = l.iter().map(|p| p[1]).collect();
    let ts: Vec<f64> = (0..97).map(|i| -1.2 + 2.4 * i as f64 / 96.0).collect();
    let mut sig = vec![];
    for k in 0..samples {
        let x = xa + (xb - xa) * k as f64 / (samples - 1).max(1) as f64;
        let y = signal::interp(x, &lx, &ly);
        let p: Vec<f32> = ts.iter().map(|t| {
            let q = h.apply([x, y + t]);
            g.sample(q[0] as f32, q[1] as f32)
        }).collect();
        let edge: Vec<f32> = p[..15].iter().chain(&p[p.len() - 15..]).copied().collect();
        let base = signal::median(&edge);
        let dip: Vec<f32> = p.iter().map(|v| (base - v).max(0.0)).collect();
        let mx = dip.iter().cloned().fold(0.0, f32::max);
        if mx < 0.03 {
            continue;
        }
        let above: Vec<usize> = (0..dip.len()).filter(|&i| dip[i] >= mx / 2.0).collect();
        let fwhm = (above[above.len() - 1] - above[0] + 1) as f64 * (ts[1] - ts[0]) * ppm_photo;
        sig.push((fwhm / 2.355) as f32);
    }
    if sig.len() < (samples / 5).max(3) {
        return None;
    }
    let s = signal::median(&sig) as f64;
    let lw = 0.2 * ppm_photo;
    Some(((s * s - lw * lw / 12.0).max(0.09)).sqrt().clamp(0.3, 3.0) as f32)
}

/// How much bigger the paper shows than its corners say, from the printed lines: the local
/// spacing of the lines against their usual spacing (per line, at its x samples), and on
/// squared paper the local width of the squares (x mm, factor). 1 where the page is as flat
/// as its corners.
fn magnification(lines: &[Vec<P2>], vlines: &[f64], kind: PaperKind) -> (Vec<Vec<f64>>, Option<(Vec<f64>, Vec<f64>)>) {
    let n = lines.len();
    let flat = lines.iter().map(|l| vec![1.0; l.len()]).collect();
    if kind == PaperKind::Blank || n < 4 || lines.iter().any(|l| l.len() != lines[0].len()) {
        return (flat, None);
    }
    let k = lines[0].len();
    let pitch = |j: usize, i: usize| -> f64 {
        if j == 0 {
            lines[1][i][1] - lines[0][i][1]
        } else if j == n - 1 {
            lines[n - 1][i][1] - lines[n - 2][i][1]
        } else {
            (lines[j + 1][i][1] - lines[j - 1][i][1]) / 2.0
        }
    };
    let all: Vec<f32> = (0..n).flat_map(|j| (1..k - 1).map(move |i| (j, i))).map(|(j, i)| pitch(j, i) as f32).collect();
    let usual = signal::median(&all) as f64;
    if usual <= 0.0 {
        return (flat, None);
    }
    let raw: Vec<Vec<f64>> = (0..n).map(|j| (0..k).map(|i| (pitch(j, i) / usual).clamp(0.8, 1.25)).collect()).collect();
    // smooth: neighbours along the line, then the median of three lines
    let along: Vec<Vec<f64>> = raw.iter().map(|r| (0..k).map(|i| {
        let a = &r[i.saturating_sub(1)..(i + 2).min(k)];
        a.iter().sum::<f64>() / a.len() as f64
    }).collect()).collect();
    let mut my: Vec<Vec<f64>> = (0..n).map(|j| (0..k).map(|i| {
        let mut v: Vec<f64> = (j.saturating_sub(1)..(j + 2).min(n)).map(|jj| along[jj][i]).collect();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v[v.len() / 2]
    }).collect()).collect();
    // within measuring noise: flat
    if my.iter().flatten().all(|m| (m - 1.0).abs() < 0.03) {
        my = flat;
    }
    let mut mx = None;
    if kind == PaperKind::Grid && vlines.len() >= 6 {
        let mut v = vlines.to_vec();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let gaps: Vec<(f64, f64)> = v.windows(2).map(|w| ((w[0] + w[1]) / 2.0, w[1] - w[0])).collect();
        let usual = signal::median(&gaps.iter().map(|g| g.1 as f32).collect::<Vec<_>>()) as f64;
        // a gap that is not one square (a missed line, the margin) says nothing
        let good: Vec<(f64, f64)> = gaps.into_iter().filter(|g| (g.1 / usual - 1.0).abs() < 0.3).collect();
        if good.len() >= 5 && good.iter().any(|g| (g.1 / usual - 1.0).abs() >= 0.03) {
            mx = Some((good.iter().map(|g| g.0).collect(), good.iter().map(|g| (g.1 / usual).clamp(0.8, 1.25)).collect()));
        }
    }
    (my, mx)
}

// --------------------------------------------------------------------------- placement mapping
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Placement {
    pub start_line: Option<usize>,
    pub left_mm: Option<f64>,
    pub right_mm: Option<f64>,
    /// MESH rows x cols offsets in full-resolution photo px
    pub mesh: Option<Vec<Vec<[f64; 2]>>>,
    pub shot: Option<usize>,
}

/// Maps writing coordinates (x mm from the writing start, line L, dy mm) to photo pixels.
/// Distances are on the paper as it is: where the page shows bigger than its corners say
/// (nearer the camera), letters and the steps between them grow with it.
pub struct PaperGeom {
    pub size: (f64, f64),
    pub h: Homography,
    pub lines: Vec<Vec<P2>>,
    pub start: usize,
    pub left: f64,
    pub right: f64,
    /// per writing line: where it crosses the (maybe bent) left and right edges, sheet x mm
    lefts: Vec<f64>,
    rights: Vec<f64>,
    pub mesh: Vec<Vec<[f64; 2]>>,
    pub pitch: f64,
    lx: Vec<Vec<f64>>,
    ly: Vec<Vec<f64>>,
    /// vertical size factor per writing line, at its x samples
    my: Vec<Vec<f64>>,
    /// per writing line: sheet x (mm) and the distance along the paper from `left` there
    xt: Vec<(Vec<f64>, Vec<f64>)>,
}

impl PaperGeom {
    pub fn new(p: &PaperMeta, pl: &Placement, scale: f64) -> Self {
        let shot = pl.shot.unwrap_or(0).min(p.shots.len() - 1);
        let s = &p.shots[shot];
        let size = geom::paper_size(&p.size);
        let h = geom::mm_to_px(&s.corners, size).then(&Homography::scale(scale, scale));
        let lines = p.writing_lines(shot);
        let ys: Vec<f32> = lines.iter().map(|l| (l.iter().map(|q| q[1]).sum::<f64>() / l.len() as f64) as f32).collect();
        let pitch = if ys.len() > 1 { signal::median(&ys.windows(2).map(|w| w[1] - w[0]).collect::<Vec<_>>()) as f64 } else { 9.0 };
        let mesh = pl.mesh.clone().unwrap_or_else(|| vec![vec![[0.0, 0.0]; MESH.1]; MESH.0]).iter().map(|r| r.iter().map(|o| [o[0] * scale, o[1] * scale]).collect()).collect();
        let lx: Vec<Vec<f64>> = lines.iter().map(|l| l.iter().map(|q| q[0]).collect()).collect();
        let (my, mx) = magnification(&lines, &s.vlines, if s.rules.is_empty() { PaperKind::Blank } else { p.kind });
        let left = pl.left_mm.unwrap_or(p.left_mm);
        let right = pl.right_mm.unwrap_or(p.right_mm);
        // each line starts and ends where it crosses the writing area's edges
        let bend = s.area_bend.clone().unwrap_or_default();
        let ly: Vec<Vec<f64>> = lines.iter().map(|l| l.iter().map(|q| q[1]).collect()).collect();
        let cross = |k: usize, base: f64| -> Vec<f64> { (0..lines.len()).map(|j| base + bend_at(&bend[k], signal::interp(base, &lx[j], &ly[j]), size.1)).collect() };
        let (lefts, rights) = (cross(0, left), cross(1, right));
        // distance along each line, 0 at the writing start: steps grow where the paper does
        let xt = (0..lines.len())
            .map(|j| {
                let left = lefts[j];
                let xs: Vec<f64> = (0..=((size.0 + 90.0) * 2.0) as usize).map(|i| left - 30.0 + i as f64 * 0.5).collect();
                let m = |x: f64| match &mx {
                    Some((vx, vm)) => signal::interp(x, vx, vm),
                    None => signal::interp(x, &lx[j], &my[j]),
                };
                let mut t = vec![0.0; xs.len()];
                for i in 1..xs.len() {
                    t[i] = t[i - 1] + 0.5 / m(xs[i] - 0.25);
                }
                let at_left = signal::interp(left, &xs, &t);
                t.iter_mut().for_each(|v| *v -= at_left);
                (xs, t)
            })
            .collect();
        PaperGeom {
            size,
            h,
            ly,
            lx,
            lines,
            start: pl.start_line.unwrap_or(p.start_line),
            left,
            right,
            lefts,
            rights,
            mesh,
            pitch,
            my,
            xt,
        }
    }

    /// The writing line nearest to line L of the page.
    fn line_at(&self, l: f64) -> Option<usize> {
        let n = self.lines.len();
        (n > 0).then(|| ((l + self.start as f64).round().max(0.0) as usize).min(n - 1))
    }

    /// A value of line L from its writing lines: between two of them (a table's border going
    /// down the page) it moves smoothly from one to the next instead of jumping half way.
    fn between(&self, l: f64, v: impl Fn(usize) -> f64) -> Option<f64> {
        let n = self.lines.len();
        if n == 0 {
            return None;
        }
        let p = (l + self.start as f64).clamp(0.0, (n - 1) as f64);
        let j = p.floor() as usize;
        let f = p - j as f64;
        Some(if f < 1e-9 || j + 1 >= n { v(j) } else { (1.0 - f) * v(j) + f * v(j + 1) })
    }

    /// Sheet x (mm) at distance `x` along the paper from the writing start, on line L.
    fn sheet_x(&self, x: f64, l: f64) -> f64 {
        self.between(l, |j| signal::interp(x, &self.xt[j].1, &self.xt[j].0)).unwrap_or(self.left + x)
    }

    /// How much bigger than its corners say the paper shows at sheet x on line L.
    fn mag(&self, l: f64, xm: f64) -> f64 {
        self.between(l, |j| signal::interp(xm, &self.lx[j], &self.my[j])).unwrap_or(1.0)
    }

    pub fn lines_per_page(&self) -> usize {
        self.lines.len().saturating_sub(self.start).max(1)
    }

    /// Writing width along the paper (the middle line's: the same on flat paper).
    pub fn width(&self) -> f64 {
        match self.line_at((self.lines.len().saturating_sub(self.start) / 2) as f64) {
            Some(j) => self.width_of(j),
            None => (self.right - self.left).max(20.0),
        }
    }

    fn width_of(&self, j: usize) -> f64 {
        signal::interp(self.rights[j].max(self.lefts[j] + 20.0), &self.xt[j].0, &self.xt[j].1).max(20.0)
    }

    /// Writing width of each line of the page, from the first (the edges may bend).
    pub fn widths(&self) -> Vec<f64> {
        (self.start.min(self.lines.len())..self.lines.len()).map(|j| self.width_of(j)).collect()
    }

    fn y_of(&self, j: isize, x: f64) -> f64 {
        let n = self.lines.len() as isize;
        if n == 0 {
            return 20.0 + j as f64 * self.pitch;
        }
        if j >= 0 && j < n {
            return signal::interp(x, &self.lx[j as usize], &self.ly[j as usize]);
        }
        let (base, off) = if j >= n { (n - 1, j - (n - 1)) } else { (0, j) };
        signal::interp(x, &self.lx[base as usize], &self.ly[base as usize]) + off as f64 * self.pitch
    }

    /// Sheet-mm y of line L (fractional allowed) at sheet-mm x.
    pub fn line_y(&self, l: f64, x: f64) -> f64 {
        let l = l + self.start as f64;
        let k = l.floor();
        let f = l - k;
        let a = self.y_of(k as isize, x);
        if f < 1e-6 { a } else { (1.0 - f) * a + f * self.y_of(k as isize + 1, x) }
    }

    pub fn to_px(&self, x: f64, l: f64, dy: f64) -> P2 {
        let xm = self.sheet_x(x, l);
        self.mm_to_px([xm, self.line_y(l, xm) + dy * self.mag(l, xm)])
    }

    pub fn mm_to_px(&self, p: P2) -> P2 {
        let mut q = self.h.apply(p);
        let (rows, cols) = (self.mesh.len(), self.mesh[0].len());
        if self.mesh.iter().flatten().any(|o| o[0] != 0.0 || o[1] != 0.0) {
            let u = (p[0] / self.size.0 * (cols - 1) as f64).clamp(0.0, (cols - 1) as f64 - 1e-6);
            let v = (p[1] / self.size.1 * (rows - 1) as f64).clamp(0.0, (rows - 1) as f64 - 1e-6);
            let (i, j) = (v as usize, u as usize);
            let (fv, fu) = (v - i as f64, u - j as f64);
            let m = &self.mesh;
            for c in 0..2 {
                q[c] += m[i][j][c] * (1.0 - fu) * (1.0 - fv) + m[i][j + 1][c] * fu * (1.0 - fv) + m[i + 1][j][c] * (1.0 - fu) * fv + m[i + 1][j + 1][c] * fu * fv;
            }
        }
        q
    }

    /// Mesh nodes in photo px without offsets (for the editor).
    pub fn base_nodes(&self) -> Vec<Vec<P2>> {
        let (rows, cols) = (self.mesh.len(), self.mesh[0].len());
        (0..rows)
            .map(|r| (0..cols).map(|c| self.h.apply([c as f64 / (cols - 1) as f64 * self.size.0, r as f64 / (rows - 1) as f64 * self.size.1])).collect())
            .collect()
    }
}

/// Downscaled flat preview of the analysis (lines in green) for thumbnails.
pub fn analysis_thumb(photo: &Rgb, shot: &Shot, size: &str, lines: &[Vec<P2>], width: usize) -> image::RgbImage {
    let sz = geom::paper_size(size);
    let ppm = width as f64 / sz.0;
    let flat = geom::rectify(photo, &geom::mm_to_px(&shot.corners, sz), sz, ppm);
    let mut im = flat.to_image();
    for l in lines {
        for k in 0..l.len().saturating_sub(1) {
            let (a, b) = (l[k], l[k + 1]);
            imageproc::drawing::draw_line_segment_mut(
                &mut im,
                ((a[0] * ppm) as f32, (a[1] * ppm) as f32),
                ((b[0] * ppm) as f32, (b[1] * ppm) as f32),
                image::Rgb([40, 170, 60]),
            );
        }
    }
    im
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A lined A5 sheet whose lines are `pitch(j)` apart, seen straight on at 4 px/mm.
    fn sheet(pitch: impl Fn(usize) -> f64) -> PaperMeta {
        let mut y = 20.0;
        let rules: Vec<Vec<P2>> = (0..20)
            .map(|j| {
                let l = (0..11).map(|i| [i as f64 * 14.8, y]).collect();
                y += pitch(j);
                l
            })
            .collect();
        let shot = Shot {
            file: String::new(),
            w: 592,
            h: 840,
            corners: [[0.0, 0.0], [592.0, 0.0], [592.0, 840.0], [0.0, 840.0]],
            found: true,
            rules,
            rules_edited: false,
            vlines: vec![],
            margin_mm: None,
            noise: 0.01,
            blur_px: 0.8,
            ppm_photo: 4.0,
            blur_map: vec![],
            area_bend: None,
        };
        let mut p = PaperMeta::new("t", "t", shot, "A5");
        p.kind = PaperKind::Lined;
        p.step = 1;
        p.start_line = 0;
        p
    }

    fn height(g: &PaperGeom, l: f64) -> f64 {
        let (a, b) = (g.to_px(40.0, l, 0.0), g.to_px(40.0, l, -3.0));
        (a[1] - b[1]).abs()
    }

    #[test]
    fn writing_grows_where_the_page_is_nearer() {
        let flat = sheet(|_| 8.0);
        let g = PaperGeom::new(&flat, &Placement::default(), 1.0);
        // flat paper: as the corners say
        assert!((height(&g, 5.0) - 12.0).abs() < 1e-6, "{}", height(&g, 5.0));
        assert!((g.to_px(30.0, 5.0, 0.0)[0] - (g.left + 30.0) * 4.0).abs() < 1e-6);
        // lines 8..12 a quarter farther apart: nearer the camera there
        let bulge = sheet(|j| if (8..12).contains(&j) { 10.0 } else { 8.0 });
        let g = PaperGeom::new(&bulge, &Placement::default(), 1.0);
        let (near, far) = (height(&g, 10.0), height(&g, 2.0));
        assert!(near / far > 1.15 && (far - 12.0).abs() < 0.6, "near {near:.2} far {far:.2}");
        // and the steps along the line with it: the same words take more of the photo
        let w = |l: f64| g.to_px(60.0, l, 0.0)[0] - g.to_px(0.0, l, 0.0)[0];
        assert!(w(10.0) / w(2.0) > 1.15, "{} {}", w(10.0), w(2.0));
    }

    #[test]
    fn a_stroke_down_the_page_has_no_steps() {
        // lines of different spacing put the same x at different photo x on each line
        let g = PaperGeom::new(&sheet(|j| if (8..12).contains(&j) { 10.0 } else { 8.0 }), &Placement::default(), 1.0);
        let x = |l: f64| g.to_px(60.0, l, 0.0)[0];
        let per_line = (6..13).map(|j| (x(j as f64 + 1.0) - x(j as f64)).abs()).fold(0.0, f64::max);
        assert!(per_line > 2.0, "{per_line}");
        // a table border from line 6 to 13: it moves a little at every step, never by a line's worth
        let step = (0..140).map(|k| (x(6.0 + (k + 1) as f64 * 0.05) - x(6.0 + k as f64 * 0.05)).abs()).fold(0.0, f64::max);
        assert!(step < 0.25 * per_line, "step {step:.2} px, between lines {per_line:.2} px");
    }
}
