//! Homographies, perspective warps and finding the paper sheet in a photo.

use nalgebra::{DMatrix, Matrix3, Vector3};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::img::{self, Gray, Rgb};

pub type P2 = [f64; 2];

pub const PAPER_SIZES: &[(&str, f64, f64)] = &[
    ("A4", 210.0, 297.0),
    ("Letter", 215.9, 279.4),
    ("A5", 148.0, 210.0),
    ("B5", 176.0, 250.0),
    ("Notebook", 170.0, 205.0),
];

/// Size in mm of a named size, or of a measured one written "WxH" (e.g. "153.2x102.1").
pub fn paper_size(name: &str) -> (f64, f64) {
    if let Some(p) = PAPER_SIZES.iter().find(|p| p.0 == name) {
        return (p.1, p.2);
    }
    name.split_once('x')
        .and_then(|(w, h)| Some((w.trim().parse::<f64>().ok()?, h.trim().parse::<f64>().ok()?)))
        .filter(|(w, h)| *w > 10.0 && *h > 10.0)
        .unwrap_or((210.0, 297.0))
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct Homography(pub [f64; 9]);

impl Homography {
    pub fn identity() -> Self {
        Homography([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0])
    }
    pub fn m(&self) -> Matrix3<f64> {
        Matrix3::from_row_slice(&self.0)
    }
    pub fn from_m(m: &Matrix3<f64>) -> Self {
        let s = if m[(2, 2)].abs() > 1e-12 { m[(2, 2)] } else { 1.0 };
        let mut a = [0.0; 9];
        for r in 0..3 {
            for c in 0..3 {
                a[r * 3 + c] = m[(r, c)] / s;
            }
        }
        Homography(a)
    }
    #[inline]
    pub fn apply(&self, p: P2) -> P2 {
        let h = &self.0;
        let w = h[6] * p[0] + h[7] * p[1] + h[8];
        [(h[0] * p[0] + h[1] * p[1] + h[2]) / w, (h[3] * p[0] + h[4] * p[1] + h[5]) / w]
    }
    pub fn inverse(&self) -> Self {
        Self::from_m(&self.m().try_inverse().unwrap_or_else(Matrix3::identity))
    }
    pub fn then(&self, next: &Homography) -> Self {
        Self::from_m(&(next.m() * self.m()))
    }
    pub fn scale(sx: f64, sy: f64) -> Self {
        Homography([sx, 0.0, 0.0, 0.0, sy, 0.0, 0.0, 0.0, 1.0])
    }
    pub fn translate(tx: f64, ty: f64) -> Self {
        Homography([1.0, 0.0, tx, 0.0, 1.0, ty, 0.0, 0.0, 1.0])
    }

    /// Normalised DLT from >= 4 correspondences src -> dst.
    pub fn fit(src: &[P2], dst: &[P2]) -> Option<Self> {
        if src.len() < 4 || src.len() != dst.len() {
            return None;
        }
        let norm = |pts: &[P2]| -> Matrix3<f64> {
            let n = pts.len() as f64;
            let cx = pts.iter().map(|p| p[0]).sum::<f64>() / n;
            let cy = pts.iter().map(|p| p[1]).sum::<f64>() / n;
            let d = pts.iter().map(|p| ((p[0] - cx).powi(2) + (p[1] - cy).powi(2)).sqrt()).sum::<f64>() / n;
            let s = if d > 1e-12 { std::f64::consts::SQRT_2 / d } else { 1.0 };
            Matrix3::new(s, 0.0, -s * cx, 0.0, s, -s * cy, 0.0, 0.0, 1.0)
        };
        let ts = norm(src);
        let td = norm(dst);
        let n = src.len();
        let mut a = DMatrix::<f64>::zeros(2 * n, 9);
        for i in 0..n {
            let s = ts * Vector3::new(src[i][0], src[i][1], 1.0);
            let d = td * Vector3::new(dst[i][0], dst[i][1], 1.0);
            let (x, y, u, v) = (s[0] / s[2], s[1] / s[2], d[0] / d[2], d[1] / d[2]);
            let r = [-x, -y, -1.0, 0.0, 0.0, 0.0, u * x, u * y, u];
            let q = [0.0, 0.0, 0.0, -x, -y, -1.0, v * x, v * y, v];
            for j in 0..9 {
                a[(2 * i, j)] = r[j];
                a[(2 * i + 1, j)] = q[j];
            }
        }
        // h = eigenvector of A^T A with the smallest eigenvalue (robust for 2n x 9).
        let ata = a.transpose() * &a;
        let eig = nalgebra::SymmetricEigen::new(ata);
        let (mut bi, mut bv) = (0, f64::MAX);
        for (i, &v) in eig.eigenvalues.iter().enumerate() {
            if v < bv {
                bv = v;
                bi = i;
            }
        }
        let h = eig.eigenvectors.column(bi);
        let hn = Matrix3::from_row_slice(h.as_slice());
        let full = td.try_inverse()? * hn * ts;
        Some(Self::from_m(&full))
    }
}

/// Paper millimetres -> photo pixels from the four sheet corners (TL, TR, BR, BL).
pub fn mm_to_px(corners: &[P2; 4], size: (f64, f64)) -> Homography {
    let (w, h) = size;
    Homography::fit(&[[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]], corners).unwrap_or_else(Homography::identity)
}

/// Output pixel -> source pixel mapping; samples src for every output pixel.
pub fn warp_rgb(src: &Rgb, dst_to_src: &Homography, w: usize, h: usize) -> Rgb {
    let mut d = vec![0f32; w * h * 3];
    d.par_chunks_mut(w * 3).enumerate().for_each(|(y, row)| {
        let mut px = [0f32; 3];
        for x in 0..w {
            let s = dst_to_src.apply([x as f64, y as f64]);
            src.sample(s[0] as f32, s[1] as f32, &mut px);
            row[x * 3..x * 3 + 3].copy_from_slice(&px);
        }
    });
    Rgb { w, h, d }
}

pub fn warp_gray(src: &Gray, dst_to_src: &Homography, w: usize, h: usize) -> Gray {
    let mut d = vec![0f32; w * h];
    d.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        for x in 0..w {
            let s = dst_to_src.apply([x as f64, y as f64]);
            row[x] = src.sample(s[0] as f32, s[1] as f32);
        }
    });
    Gray { w, h, d }
}

/// Flat view of the sheet at `ppm` pixels per millimetre.
pub fn rectify(photo: &Rgb, mm2px: &Homography, size: (f64, f64), ppm: f64) -> Rgb {
    let m = Homography::scale(1.0 / ppm, 1.0 / ppm).then(mm2px);
    warp_rgb(photo, &m, (size.0 * ppm).round() as usize, (size.1 * ppm).round() as usize)
}

pub fn order_corners(pts: &[P2]) -> [P2; 4] {
    let cx = pts.iter().map(|p| p[0]).sum::<f64>() / pts.len() as f64;
    let cy = pts.iter().map(|p| p[1]).sum::<f64>() / pts.len() as f64;
    let mut v: Vec<P2> = pts.to_vec();
    v.sort_by(|a, b| (a[1] - cy).atan2(a[0] - cx).partial_cmp(&(b[1] - cy).atan2(b[0] - cx)).unwrap());
    let k = (0..v.len()).min_by(|&a, &b| (v[a][0] + v[a][1]).partial_cmp(&(v[b][0] + v[b][1])).unwrap()).unwrap_or(0);
    v.rotate_left(k);
    [v[0], v[1], v[2], v[3]]
}

pub fn guess_size(c: &[P2; 4]) -> &'static str {
    let d = |a: P2, b: P2| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt();
    let wpx = 0.5 * (d(c[0], c[1]) + d(c[3], c[2]));
    let hpx = 0.5 * (d(c[0], c[3]) + d(c[1], c[2]));
    let r = wpx.max(hpx) / wpx.min(hpx).max(1.0);
    PAPER_SIZES
        .iter()
        .min_by(|a, b| ((a.2 / a.1) - r).abs().partial_cmp(&((b.2 / b.1) - r).abs()).unwrap())
        .map(|p| p.0)
        .unwrap_or("A4")
}

fn otsu(v: &[f32]) -> f32 {
    let mut hist = [0usize; 256];
    for &x in v {
        hist[(x.clamp(0.0, 1.0) * 255.0) as usize] += 1;
    }
    let total = v.len() as f64;
    let sum: f64 = hist.iter().enumerate().map(|(i, &c)| i as f64 * c as f64).sum();
    let (mut wb, mut sb, mut best, mut thr) = (0.0, 0.0, 0.0, 0usize);
    for (i, &c) in hist.iter().enumerate() {
        wb += c as f64;
        if wb == 0.0 {
            continue;
        }
        let wf = total - wb;
        if wf == 0.0 {
            break;
        }
        sb += i as f64 * c as f64;
        let mb = sb / wb;
        let mf = (sum - sb) / wf;
        let between = wb * wf * (mb - mf).powi(2);
        if between > best {
            best = between;
            thr = i;
        }
    }
    thr as f32 / 255.0
}

/// Corners (TL, TR, BR, BL) of the paper sheet, and whether its edges were actually found.
pub fn detect_sheet(photo: &Rgb) -> ([P2; 4], bool) {
    let full = [[0.0, 0.0], [photo.w as f64 - 1.0, 0.0], [photo.w as f64 - 1.0, photo.h as f64 - 1.0], [0.0, photo.h as f64 - 1.0]];
    let s = 900.0 / photo.w.max(photo.h) as f64;
    let (w, h) = (((photo.w as f64) * s) as usize, ((photo.h as f64) * s) as usize);
    let small = photo.resize(w, h);
    // Paper: bright and nearly colourless.
    let lum: Vec<f32> = small.d.chunks(3).map(|p| 0.299 * p[0] + 0.587 * p[1] + 0.114 * p[2]).collect();
    let chroma: Vec<f32> = small.d.chunks(3).map(|p| {
        let m = p[0].max(p[1]).max(p[2]);
        let n = p[0].min(p[1]).min(p[2]);
        m - n
    }).collect();
    let lumg = img::blur(&Gray { w, h, d: lum }, 1.2);
    let t = otsu(&lumg.d);
    let mask: Vec<bool> = lumg.d.iter().zip(&chroma).map(|(&l, &c)| l > t && c < 0.16).collect();
    // close then open to remove text and specks
    let g = Gray { w, h, d: mask.iter().map(|&b| if b { 1.0 } else { 0.0 }).collect() };
    let g = img::minmax(&img::minmax(&g, 15, true), 15, false);
    let g = img::minmax(&img::minmax(&g, 9, false), 9, true);
    let mask: Vec<bool> = g.d.iter().map(|&v| v > 0.5).collect();
    let (lab, comps) = img::components(&mask, w, h);
    let Some((best, c)) = comps.iter().enumerate().skip(1).max_by_key(|(_, c)| c.area) else {
        return (full, false);
    };
    let frac = c.area as f64 / (w * h) as f64;
    if frac < 0.12 {
        return (full, false);
    }
    if frac > 0.97 {
        return (full, false);
    }
    // Boundary pixels of the component -> convex hull -> 4-gon.
    let mut pts = Vec::new();
    for y in c.y0..c.y1 {
        for x in c.x0..c.x1 {
            if lab[y * w + x] as usize != best {
                continue;
            }
            let edge = x == 0 || y == 0 || x + 1 == w || y + 1 == h
                || lab[y * w + x - 1] as usize != best || lab[y * w + x + 1] as usize != best
                || lab[(y - 1) * w + x] as usize != best || lab[(y + 1) * w + x] as usize != best;
            if edge {
                pts.push(imageproc::point::Point::new(x as i32, y as i32));
            }
        }
    }
    let hull = imageproc::geometry::convex_hull(pts);
    if hull.len() < 4 {
        return (full, false);
    }
    let peri: f64 = (0..hull.len())
        .map(|i| {
            let (a, b) = (hull[i], hull[(i + 1) % hull.len()]);
            (((a.x - b.x).pow(2) + (a.y - b.y).pow(2)) as f64).sqrt()
        })
        .sum();
    let hf: Vec<imageproc::point::Point<f64>> = hull.iter().map(|p| imageproc::point::Point::new(p.x as f64, p.y as f64)).collect();
    let mut quad: Option<Vec<P2>> = None;
    for i in 0..15 {
        let eps = 0.01 + 0.005 * i as f64;
        let mut closed = hf.clone();
        closed.push(hf[0]);
        let ap = imageproc::geometry::approximate_polygon_dp(&closed, eps * peri, true);
        let mut ap: Vec<P2> = ap.iter().map(|p| [p.x, p.y]).collect();
        if ap.len() > 1 && ap[0] == ap[ap.len() - 1] {
            ap.pop();
        }
        if ap.len() == 4 {
            quad = Some(ap);
            break;
        }
    }
    let quad = match quad {
        Some(q) => q,
        None => {
            // fall back: extreme points along the diagonals
            let ext = |f: &dyn Fn(&imageproc::point::Point<f64>) -> f64, max: bool| -> P2 {
                let p = if max {
                    hf.iter().max_by(|a, b| f(a).partial_cmp(&f(b)).unwrap()).unwrap()
                } else {
                    hf.iter().min_by(|a, b| f(a).partial_cmp(&f(b)).unwrap()).unwrap()
                };
                [p.x, p.y]
            };
            vec![ext(&|p| p.x + p.y, false), ext(&|p| p.x - p.y, true), ext(&|p| p.x + p.y, true), ext(&|p| p.x - p.y, false)]
        }
    };
    // a close-up cuts the sheet off at the frame: then it is not the outline of a whole sheet
    let on_frame = quad.iter().filter(|p| p[0] < 0.015 * w as f64 || p[1] < 0.015 * h as f64 || p[0] > 0.985 * w as f64 || p[1] > 0.985 * h as f64).count();
    if on_frame >= 2 {
        return (full, false);
    }
    let scaled: Vec<P2> = quad.iter().map(|p| [p[0] / s, p[1] / s]).collect();
    (order_corners(&scaled), true)
}

/// Spacing (px) of the printed lines of squared paper, from the regular vertical grid lines.
/// Measured in horizontal bands (a tilted photo smears one long profile) and averaged.
pub fn grid_pitch(photo: &Rgb) -> Option<f64> {
    // tuned for photos about 1000 px wide: a big phone photo is looked at smaller
    let k = (photo.w.max(photo.h) as f64 / 1300.0).ceil().max(1.0) as usize;
    let small = if k > 1 { photo.resize(photo.w / k, photo.h / k) } else { photo.clone() };
    let f = photo.w as f64 / small.w as f64;
    grid_pitch_at(&small).or_else(|| grid_pitch_profile(&small)).map(|p| p * f)
}

/// Squares measured another way, for a page photographed close and a little bent (the lines
/// then drift across a whole row): the repeat of the grid in narrow bands, down and across,
/// which must agree.
fn grid_pitch_profile(photo: &Rgb) -> Option<f64> {
    let ch = photo.min_channel();
    let bg = crate::matte::background(&ch, (ch.w.max(ch.h) as f32 / 250.0).max(2.0));
    let d = Gray { w: ch.w, h: ch.h, d: ch.d.iter().zip(&bg.d).map(|(c, b)| (1.0 - c / b).clamp(0.0, 0.2)).collect() };
    let hi = (ch.w.min(ch.h) / 8).clamp(12, 90);
    let (a, b) = (repeat_pitch(&d, true, 8, hi)?, repeat_pitch(&d, false, 8, hi)?);
    let (lo, hi) = (a.min(b), a.max(b));
    if hi - lo < 0.15 * lo {
        Some((a + b) / 2.0)
    } else if (hi / lo - 2.0).abs() < 0.15 {
        Some(lo)
    } else {
        None
    }
}

/// Spacing (px) of evenly repeated lines in a darkness map: `rows` = lines running across (the
/// repeat down the page), else lines running down. The shortest strong repeat, measured in
/// bands (a line drifting a little over the whole width still repeats within a band); a
/// multiple of the spacing can repeat as strongly. None without a clear repeat in lo..=hi px.
pub fn repeat_pitch(d: &Gray, rows: bool, lo: usize, hi: usize) -> Option<f64> {
    let (len, across) = if rows { (d.h, d.w) } else { (d.w, d.h) };
    if lo < 2 || hi + 2 >= len || across < 8 {
        return None;
    }
    let at = |i: usize, j: usize| if rows { d.d[i * d.w + j] } else { d.d[j * d.w + i] };
    // narrow bands, averaged: a line tilted a little still makes a peak (lines running the other
    // way only lift every row alike)
    let bands = (across / (2 * hi).max(16)).clamp(4, 16);
    let mut ac = vec![0f64; hi + 2];
    let mut used = 0;
    for b in 0..bands {
        let (j0, j1) = (b * across / bands, (b + 1) * across / bands);
        let prof: Vec<f32> = (0..len).map(|i| (j0..j1).map(|j| at(i, j)).sum::<f32>() / (j1 - j0).max(1) as f32).collect();
        let base = crate::signal::gauss1d(&prof, hi as f32 / 2.0);
        // the photo's ends (desk, a shadow at the page's edge) left out
        let edge = len / 16;
        let z: Vec<f64> = prof.iter().zip(&base).enumerate().map(|(i, (a, b))| if i < edge || i + edge >= len { 0.0 } else { (a - b) as f64 }).collect();
        let a0: f64 = z.iter().map(|v| v * v).sum();
        if a0 <= 1e-12 {
            continue;
        }
        for (lag, a) in ac.iter_mut().enumerate() {
            *a += (0..len - lag).map(|i| z[i] * z[i + lag]).sum::<f64>() / a0;
        }
        used += 1;
    }
    if used == 0 {
        return None;
    }
    ac.iter_mut().for_each(|a| *a /= used as f64);
    let peaks: Vec<usize> = (lo..=hi).filter(|&l| ac[l] > ac[l - 1] && ac[l] >= ac[l + 1] && ac[l] > 0.2).collect();
    let best = peaks.iter().map(|&l| ac[l]).fold(0.0, f64::max);
    let l = *peaks.iter().find(|&&l| ac[l] >= 0.6 * best)?;
    let (a, b, c) = (ac[l - 1], ac[l], ac[l + 1]);
    let den = a - 2.0 * b + c;
    Some(l as f64 + if den.abs() > 1e-9 { (0.5 * (a - c) / den).clamp(-0.5, 0.5) } else { 0.0 })
}

fn grid_pitch_at(photo: &Rgb) -> Option<f64> {
    let ch = photo.min_channel();
    let (w, h) = (ch.w, ch.h);
    let bg = crate::matte::background(&ch, (w.max(h) as f32 / 250.0).max(2.0));
    // faint marks only: the grid, not the (darker) writing
    let d: Vec<f32> = ch.d.iter().zip(&bg.d).map(|(c, b)| (1.0 - c / b).clamp(0.0, 0.2)).collect();
    let max_lag = (w / 6).min(250);
    if max_lag < 12 {
        return None;
    }
    let bands = 8;
    let mut acf = vec![0f64; max_lag + 1];
    for k in 0..bands {
        let (y0, y1) = (k * h / bands, (k + 1) * h / bands);
        let col: Vec<f32> = (0..w).map(|x| (y0..y1).map(|y| d[y * w + x]).sum()).collect();
        let smooth = crate::signal::gauss1d(&col, 6.0);
        let p: Vec<f64> = col.iter().zip(&smooth).map(|(a, b)| (a - b) as f64).collect();
        let e: f64 = p.iter().map(|v| v * v).sum();
        if e <= 0.0 {
            continue;
        }
        for (lag, a) in acf.iter_mut().enumerate().skip(1) {
            *a += (0..w - lag).map(|i| p[i] * p[i + lag]).sum::<f64>() / e / bands as f64;
        }
    }
    let (best, r) = (8..=max_lag).map(|l| (l, acf[l])).max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())?;
    // a real grid repeats: strong at the pitch and again near twice the pitch (a photo taken
    // at an angle has squares of slightly different sizes, so not exactly twice)
    let k = (best / 10).max(1);
    if r < 0.2 || best * 2 + k > max_lag {
        return None;
    }
    let again = (best * 2 - k..=best * 2 + k).map(|l| acf[l]).fold(f64::MIN, f64::max);
    if again < 0.45 * r {
        return None;
    }
    let (l, c, rr) = (acf[best - 1], acf[best], acf[best + 1]);
    let den = l - 2.0 * c + rr;
    let off = if den.abs() > 1e-9 { (0.5 * (l - rr) / den).clamp(-0.5, 0.5) } else { 0.0 };
    Some(best as f64 + off)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A big phone photo of squared paper taken at a slight angle (squares of 100 px at the top,
    /// 104 px at the bottom): the grid is found, its squares measured.
    #[test]
    fn grid_of_a_big_photo_at_an_angle() {
        let (w, h) = (2600usize, 3400usize);
        let mut d = vec![0.92f32; w * h * 3];
        for y in 0..h {
            let pitch = 100.0 + 4.0 * y as f64 / h as f64;
            for x in 0..w {
                let on = (x as f64 % pitch) < 3.0 || y % 100 < 3;
                if on {
                    let i = (y * w + x) * 3;
                    d[i] = 0.55;
                    d[i + 1] = 0.75;
                    d[i + 2] = 0.85;
                }
            }
        }
        let p = grid_pitch(&Rgb { w, h, d }).expect("grid found");
        assert!((p - 102.0).abs() < 4.0, "{p}");
    }

    #[test]
    fn homography_roundtrip() {
        let src = [[0.0, 0.0], [210.0, 0.0], [210.0, 297.0], [0.0, 297.0], [100.0, 50.0]];
        let truth = Homography([2.1, 0.3, 120.0, -0.1, 2.4, 80.0, 0.0002, -0.0001, 1.0]);
        let dst: Vec<P2> = src.iter().map(|&p| truth.apply(p)).collect();
        let h = Homography::fit(&src, &dst).unwrap();
        for p in src {
            let a = h.apply(p);
            let b = truth.apply(p);
            assert!((a[0] - b[0]).abs() < 1e-6 && (a[1] - b[1]).abs() < 1e-6);
        }
        let back = h.inverse().apply(h.apply([33.0, 44.0]));
        assert!((back[0] - 33.0).abs() < 1e-6);
    }
}
