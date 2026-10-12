//! Minimal float image buffers and the filters the engine needs (parallel with rayon).

use rayon::prelude::*;

/// Single-channel f32 image, row-major.
#[derive(Clone, Debug, Default)]
pub struct Gray {
    pub w: usize,
    pub h: usize,
    pub d: Vec<f32>,
}

/// Interleaved RGB f32 image, values usually in [0, 1].
#[derive(Clone, Debug, Default)]
pub struct Rgb {
    pub w: usize,
    pub h: usize,
    pub d: Vec<f32>,
}

impl Gray {
    pub fn new(w: usize, h: usize, v: f32) -> Self {
        Self { w, h, d: vec![v; w * h] }
    }
    #[inline]
    pub fn at(&self, x: usize, y: usize) -> f32 {
        self.d[y * self.w + x]
    }
    #[inline]
    pub fn set(&mut self, x: usize, y: usize, v: f32) {
        self.d[y * self.w + x] = v;
    }
    /// Bilinear sample with zero outside.
    #[inline]
    pub fn sample(&self, x: f32, y: f32) -> f32 {
        if x < -1.0 || y < -1.0 || x > self.w as f32 || y > self.h as f32 {
            return 0.0;
        }
        let x0 = x.floor();
        let y0 = y.floor();
        let fx = x - x0;
        let fy = y - y0;
        let (x0, y0) = (x0 as isize, y0 as isize);
        let g = |xi: isize, yi: isize| -> f32 {
            if xi < 0 || yi < 0 || xi >= self.w as isize || yi >= self.h as isize {
                0.0
            } else {
                self.d[yi as usize * self.w + xi as usize]
            }
        };
        let a = g(x0, y0) * (1.0 - fx) + g(x0 + 1, y0) * fx;
        let b = g(x0, y0 + 1) * (1.0 - fx) + g(x0 + 1, y0 + 1) * fx;
        a * (1.0 - fy) + b * fy
    }
    pub fn crop(&self, x0: usize, y0: usize, x1: usize, y1: usize) -> Gray {
        let (x1, y1) = (x1.min(self.w), y1.min(self.h));
        let (x0, y0) = (x0.min(x1), y0.min(y1));
        let w = x1 - x0;
        let mut d = Vec::with_capacity(w * (y1 - y0));
        for y in y0..y1 {
            d.extend_from_slice(&self.d[y * self.w + x0..y * self.w + x1]);
        }
        Gray { w, h: y1 - y0, d }
    }
    pub fn map(&self, f: impl Fn(f32) -> f32 + Sync) -> Gray {
        Gray { w: self.w, h: self.h, d: self.d.par_iter().map(|&v| f(v)).collect() }
    }
    pub fn to_u8(&self) -> image::GrayImage {
        image::GrayImage::from_raw(
            self.w as u32,
            self.h as u32,
            self.d.iter().map(|v| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8).collect(),
        )
        .expect("size")
    }
    pub fn from_u8(img: &image::GrayImage) -> Gray {
        Gray {
            w: img.width() as usize,
            h: img.height() as usize,
            d: img.as_raw().iter().map(|&v| v as f32 / 255.0).collect(),
        }
    }
    /// Mean of f x f blocks (fast, parallel).
    pub fn downsample(&self, f: usize) -> Gray {
        let (w, h) = (self.w / f, self.h / f);
        let mut d = vec![0f32; w * h];
        let inv = 1.0 / (f * f) as f32;
        d.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
            for (x, o) in row.iter_mut().enumerate() {
                let mut s = 0.0;
                for yy in y * f..(y + 1) * f {
                    let base = yy * self.w + x * f;
                    s += self.d[base..base + f].iter().sum::<f32>();
                }
                *o = s * inv;
            }
        });
        Gray { w, h, d }
    }

    /// Bilinear upscale to (w, h) (fast, parallel). Pixel centres are aligned.
    pub fn upsample(&self, w: usize, h: usize) -> Gray {
        let (sx, sy) = (self.w as f32 / w as f32, self.h as f32 / h as f32);
        let mut d = vec![0f32; w * h];
        d.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
            let fy = ((y as f32 + 0.5) * sy - 0.5).clamp(0.0, (self.h - 1) as f32);
            let y0 = fy.floor() as usize;
            let y1 = (y0 + 1).min(self.h - 1);
            let ty = fy - y0 as f32;
            for (x, o) in row.iter_mut().enumerate() {
                let fx = ((x as f32 + 0.5) * sx - 0.5).clamp(0.0, (self.w - 1) as f32);
                let x0 = fx.floor() as usize;
                let x1 = (x0 + 1).min(self.w - 1);
                let tx = fx - x0 as f32;
                let a = self.d[y0 * self.w + x0] * (1.0 - tx) + self.d[y0 * self.w + x1] * tx;
                let b = self.d[y1 * self.w + x0] * (1.0 - tx) + self.d[y1 * self.w + x1] * tx;
                *o = a * (1.0 - ty) + b * ty;
            }
        });
        Gray { w, h, d }
    }

    pub fn resize(&self, w: usize, h: usize) -> Gray {
        let img = image::ImageBuffer::<image::Luma<f32>, Vec<f32>>::from_raw(self.w as u32, self.h as u32, self.d.clone()).expect("size");
        let filter = if w < self.w { image::imageops::FilterType::Triangle } else { image::imageops::FilterType::CatmullRom };
        let r = image::imageops::resize(&img, w as u32, h as u32, filter);
        Gray { w, h, d: r.into_raw() }
    }
}

impl Rgb {
    pub fn from_image(img: &image::RgbImage) -> Rgb {
        Rgb {
            w: img.width() as usize,
            h: img.height() as usize,
            d: img.as_raw().par_iter().map(|&v| v as f32 / 255.0).collect(),
        }
    }
    pub fn to_image(&self) -> image::RgbImage {
        image::RgbImage::from_raw(
            self.w as u32,
            self.h as u32,
            self.d.par_iter().map(|v| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8).collect(),
        )
        .expect("size")
    }
    pub fn channel(&self, c: usize) -> Gray {
        Gray { w: self.w, h: self.h, d: self.d.par_chunks(3).map(|p| p[c]).collect() }
    }
    pub fn min_channel(&self) -> Gray {
        Gray { w: self.w, h: self.h, d: self.d.par_chunks(3).map(|p| p[0].min(p[1]).min(p[2])).collect() }
    }
    pub fn mean_channel(&self) -> Gray {
        Gray { w: self.w, h: self.h, d: self.d.par_chunks(3).map(|p| (p[0] + p[1] + p[2]) / 3.0).collect() }
    }
    pub fn resize(&self, w: usize, h: usize) -> Rgb {
        let img = image::ImageBuffer::<image::Rgb<f32>, Vec<f32>>::from_raw(self.w as u32, self.h as u32, self.d.clone()).expect("size");
        let filter = if w < self.w { image::imageops::FilterType::Triangle } else { image::imageops::FilterType::CatmullRom };
        let r = image::imageops::resize(&img, w as u32, h as u32, filter);
        Rgb { w, h, d: r.into_raw() }
    }
    #[inline]
    pub fn sample(&self, x: f32, y: f32, out: &mut [f32; 3]) {
        let xc = x.clamp(0.0, (self.w - 1) as f32);
        let yc = y.clamp(0.0, (self.h - 1) as f32);
        let x0 = xc.floor() as usize;
        let y0 = yc.floor() as usize;
        let x1 = (x0 + 1).min(self.w - 1);
        let y1 = (y0 + 1).min(self.h - 1);
        let fx = xc - x0 as f32;
        let fy = yc - y0 as f32;
        for c in 0..3 {
            let p = |xx: usize, yy: usize| self.d[(yy * self.w + xx) * 3 + c];
            let a = p(x0, y0) * (1.0 - fx) + p(x1, y0) * fx;
            let b = p(x0, y1) * (1.0 - fx) + p(x1, y1) * fx;
            out[c] = a * (1.0 - fy) + b * fy;
        }
    }
}

// ---------------------------------------------------------------- blur
fn kernel(sigma: f32) -> Vec<f32> {
    let r = (3.0 * sigma).ceil().max(1.0) as isize;
    let mut k: Vec<f32> = (-r..=r).map(|i| (-(i * i) as f32 / (2.0 * sigma * sigma)).exp()).collect();
    let s: f32 = k.iter().sum();
    k.iter_mut().for_each(|v| *v /= s);
    k
}

/// Gaussian blur. Large sigmas use three box passes (same result to the eye, O(1) per pixel).
pub fn blur(src: &Gray, sigma: f32) -> Gray {
    if sigma <= 0.05 {
        return src.clone();
    }
    if sigma > 6.0 {
        return box_gauss(src, sigma);
    }
    let k = kernel(sigma);
    let r = (k.len() / 2) as isize;
    let (w, h) = (src.w, src.h);
    let mut tmp = vec![0f32; w * h];
    tmp.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        let s = &src.d[y * w..(y + 1) * w];
        for x in 0..w {
            let mut acc = 0.0;
            for (i, kv) in k.iter().enumerate() {
                let xx = (x as isize + i as isize - r).clamp(0, w as isize - 1) as usize;
                acc += kv * s[xx];
            }
            row[x] = acc;
        }
    });
    let mut out = vec![0f32; w * h];
    out.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        for x in 0..w {
            let mut acc = 0.0;
            for (i, kv) in k.iter().enumerate() {
                let yy = (y as isize + i as isize - r).clamp(0, h as isize - 1) as usize;
                acc += kv * tmp[yy * w + x];
            }
            row[x] = acc;
        }
    });
    Gray { w, h, d: out }
}

fn box_gauss(src: &Gray, sigma: f32) -> Gray {
    // Widths of 3 box filters approximating a Gaussian (Kovesi).
    let n = 3.0f32;
    let wi = ((12.0 * sigma * sigma / n) + 1.0).sqrt().floor();
    let wl = if (wi as i32) % 2 == 0 { wi - 1.0 } else { wi };
    let wu = wl + 2.0;
    let m = ((12.0 * sigma * sigma - n * wl * wl - 4.0 * n * wl - 3.0 * n) / (-4.0 * wl - 4.0)).round();
    let mut cur = src.clone();
    for i in 0..3 {
        let size = if (i as f32) < m { wl } else { wu };
        let r = ((size - 1.0) / 2.0).max(1.0) as usize;
        cur = box_blur(&cur, r);
    }
    cur
}

pub fn box_blur(src: &Gray, r: usize) -> Gray {
    let (w, h) = (src.w, src.h);
    let mut tmp = vec![0f32; w * h];
    tmp.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        let s = &src.d[y * w..(y + 1) * w];
        running_mean(s, row, r, 1);
    });
    let mut out = vec![0f32; w * h];
    // columns: transpose-free by processing column strips
    let cols: Vec<Vec<f32>> = (0..w)
        .into_par_iter()
        .map(|x| {
            let col: Vec<f32> = (0..h).map(|y| tmp[y * w + x]).collect();
            let mut o = vec![0f32; h];
            running_mean(&col, &mut o, r, 1);
            o
        })
        .collect();
    for (x, col) in cols.iter().enumerate() {
        for y in 0..h {
            out[y * w + x] = col[y];
        }
    }
    Gray { w, h, d: out }
}

fn running_mean(s: &[f32], o: &mut [f32], r: usize, _step: usize) {
    let n = s.len();
    if n == 0 {
        return;
    }
    let get = |i: isize| s[i.clamp(0, n as isize - 1) as usize];
    let win = (2 * r + 1) as f32;
    let mut acc: f32 = (-(r as isize)..=r as isize).map(get).sum();
    for i in 0..n {
        o[i] = acc / win;
        acc += get(i as isize + r as isize + 1) - get(i as isize - r as isize);
    }
}

// ---------------------------------------------------------------- min / max filters (van Herk / Gil-Werman)
fn run_minmax(s: &[f32], o: &mut [f32], k: usize, max: bool) {
    let n = s.len();
    if n == 0 || k <= 1 {
        o.copy_from_slice(s);
        return;
    }
    let r = k / 2;
    let pad = |i: isize| s[i.clamp(0, n as isize - 1) as usize];
    let len = n + 2 * r;
    let ext: Vec<f32> = (0..len).map(|i| pad(i as isize - r as isize)).collect();
    let mut g = vec![0f32; len];
    let mut hb = vec![0f32; len];
    let op = |a: f32, b: f32| if max { a.max(b) } else { a.min(b) };
    for i in 0..len {
        g[i] = if i % k == 0 { ext[i] } else { op(g[i - 1], ext[i]) };
    }
    for i in (0..len).rev() {
        hb[i] = if i == len - 1 || (i + 1) % k == 0 { ext[i] } else { op(hb[i + 1], ext[i]) };
    }
    for i in 0..n {
        // window [i, i + k - 1] in ext coordinates
        let a = i;
        let b = (i + k - 1).min(len - 1);
        o[i] = op(hb[a], g[b]);
    }
}

/// Rectangular grey dilation (max) or erosion (min) with window k x k.
pub fn minmax(src: &Gray, k: usize, max: bool) -> Gray {
    let (w, h) = (src.w, src.h);
    let mut tmp = vec![0f32; w * h];
    tmp.par_chunks_mut(w).enumerate().for_each(|(y, row)| run_minmax(&src.d[y * w..(y + 1) * w], row, k, max));
    let cols: Vec<Vec<f32>> = (0..w)
        .into_par_iter()
        .map(|x| {
            let col: Vec<f32> = (0..h).map(|y| tmp[y * w + x]).collect();
            let mut o = vec![0f32; h];
            run_minmax(&col, &mut o, k, max);
            o
        })
        .collect();
    let mut out = vec![0f32; w * h];
    for (x, col) in cols.iter().enumerate() {
        for y in 0..h {
            out[y * w + x] = col[y];
        }
    }
    Gray { w, h, d: out }
}

/// Binary dilation of a mask with a square of radius r.
pub fn dilate_mask(m: &[bool], w: usize, h: usize, r: usize) -> Vec<bool> {
    if r == 0 {
        return m.to_vec();
    }
    let g = Gray { w, h, d: m.iter().map(|&b| if b { 1.0 } else { 0.0 }).collect() };
    minmax(&g, 2 * r + 1, true).d.iter().map(|&v| v > 0.5).collect()
}

// ---------------------------------------------------------------- connected components
#[derive(Clone, Copy, Debug, Default)]
pub struct Comp {
    pub area: usize,
    pub x0: usize,
    pub y0: usize,
    pub x1: usize, // exclusive
    pub y1: usize, // exclusive
    pub cx: f32,
    pub cy: f32,
}

impl Comp {
    pub fn w(&self) -> usize {
        self.x1 - self.x0
    }
    pub fn h(&self) -> usize {
        self.y1 - self.y0
    }
}

/// 8-connected labelling. Labels start at 1 (0 = background). comps[0] is unused.
pub fn components(mask: &[bool], w: usize, h: usize) -> (Vec<u32>, Vec<Comp>) {
    let mut lab = vec![0u32; w * h];
    let mut parent: Vec<u32> = vec![0];
    fn find(p: &mut [u32], mut x: u32) -> u32 {
        while p[x as usize] != x {
            p[x as usize] = p[p[x as usize] as usize];
            x = p[x as usize];
        }
        x
    }
    for y in 0..h {
        for x in 0..w {
            if !mask[y * w + x] {
                continue;
            }
            let mut best = 0u32;
            let mut neigh = [0u32; 4];
            let mut n = 0;
            let cand = [(x as isize - 1, y as isize), (x as isize - 1, y as isize - 1), (x as isize, y as isize - 1), (x as isize + 1, y as isize - 1)];
            for (xx, yy) in cand {
                if xx >= 0 && yy >= 0 && (xx as usize) < w {
                    let l = lab[yy as usize * w + xx as usize];
                    if l != 0 {
                        neigh[n] = l;
                        n += 1;
                        best = if best == 0 { l } else { best.min(l) };
                    }
                }
            }
            if best == 0 {
                let id = parent.len() as u32;
                parent.push(id);
                lab[y * w + x] = id;
            } else {
                lab[y * w + x] = best;
                for &l in &neigh[..n] {
                    let a = find(&mut parent, l);
                    let b = find(&mut parent, best);
                    if a != b {
                        let (lo, hi) = (a.min(b), a.max(b));
                        parent[hi as usize] = lo;
                    }
                }
            }
        }
    }
    // compact labels
    let mut remap = vec![0u32; parent.len()];
    let mut next = 1u32;
    for i in 1..parent.len() {
        let r = find(&mut parent, i as u32);
        if remap[r as usize] == 0 {
            remap[r as usize] = next;
            next += 1;
        }
        remap[i] = remap[r as usize];
    }
    let mut comps = vec![Comp { x0: usize::MAX, y0: usize::MAX, ..Default::default() }; next as usize];
    let mut sx = vec![0f64; next as usize];
    let mut sy = vec![0f64; next as usize];
    for y in 0..h {
        for x in 0..w {
            let l = lab[y * w + x];
            if l == 0 {
                continue;
            }
            let nl = remap[l as usize];
            lab[y * w + x] = nl;
            let c = &mut comps[nl as usize];
            c.area += 1;
            c.x0 = c.x0.min(x);
            c.y0 = c.y0.min(y);
            c.x1 = c.x1.max(x + 1);
            c.y1 = c.y1.max(y + 1);
            sx[nl as usize] += x as f64;
            sy[nl as usize] += y as f64;
        }
    }
    for i in 1..comps.len() {
        let a = comps[i].area.max(1) as f64;
        comps[i].cx = (sx[i] / a) as f32;
        comps[i].cy = (sy[i] / a) as f32;
    }
    comps[0] = Comp::default();
    (lab, comps)
}

// ---------------------------------------------------------------- color
#[inline]
pub fn srgb_to_linear(x: f32) -> f32 {
    if x <= 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) }
}
#[inline]
pub fn linear_to_srgb(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    if x <= 0.0031308 { x * 12.92 } else { 1.055 * x.powf(1.0 / 2.4) - 0.055 }
}

/// Fast LUT-based sRGB->linear for whole images.
pub fn to_linear(img: &Rgb) -> Rgb {
    let lut: Vec<f32> = (0..=4096).map(|i| srgb_to_linear(i as f32 / 4096.0)).collect();
    Rgb { w: img.w, h: img.h, d: img.d.par_iter().map(|&v| lut[(v.clamp(0.0, 1.0) * 4096.0) as usize]).collect() }
}
pub fn to_srgb(img: &Rgb) -> Rgb {
    let lut: Vec<f32> = (0..=8192).map(|i| linear_to_srgb(i as f32 / 8192.0)).collect();
    Rgb { w: img.w, h: img.h, d: img.d.par_iter().map(|&v| lut[(v.clamp(0.0, 1.0) * 8192.0) as usize]).collect() }
}
