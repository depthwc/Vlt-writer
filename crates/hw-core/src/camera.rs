//! A new random "shot" of the same scene: camera rotation (exact for a flat scene), uneven
//! light, soft shadow, white balance, sensor noise, phone-style sharpening.

use rand::Rng;
use rand_distr::StandardNormal;
use rayon::prelude::*;

use crate::geom::{Homography, P2};
use crate::img::{self, Gray, Rgb};

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ShotStyle {
    /// 0..1 strength of every random effect
    pub strength: f32,
    pub angle: bool,
    /// the camera turns at most this far (degrees, about each axis)
    pub angle_max: f32,
    pub light: bool,
    pub shadow: bool,
    pub noise: bool,
}

impl Default for ShotStyle {
    fn default() -> Self {
        ShotStyle { strength: 1.0, angle: true, angle_max: 4.0, light: true, shadow: true, noise: true }
    }
}

fn rot(ax: f64, ay: f64, az: f64) -> nalgebra::Matrix3<f64> {
    let rx = nalgebra::Matrix3::new(1.0, 0.0, 0.0, 0.0, ax.cos(), -ax.sin(), 0.0, ax.sin(), ax.cos());
    let ry = nalgebra::Matrix3::new(ay.cos(), 0.0, ay.sin(), 0.0, 1.0, 0.0, -ay.sin(), 0.0, ay.cos());
    let rz = nalgebra::Matrix3::new(az.cos(), -az.sin(), 0.0, az.sin(), az.cos(), 0.0, 0.0, 0.0, 1.0);
    rz * ry * rx
}

/// K R K^-1, then a centred crop back to a full frame (same aspect).
fn view_matrix(w: usize, h: usize, deg: [f64; 3]) -> Homography {
    let f = 0.8 * w.max(h) as f64;
    let k = nalgebra::Matrix3::new(f, 0.0, w as f64 / 2.0, 0.0, f, h as f64 / 2.0, 0.0, 0.0, 1.0);
    let hv = Homography::from_m(&(k * rot(deg[0].to_radians(), deg[1].to_radians(), deg[2].to_radians()) * k.try_inverse().unwrap()));
    let corners: Vec<P2> = [[0.0, 0.0], [w as f64, 0.0], [w as f64, h as f64], [0.0, h as f64]].iter().map(|&p| hv.apply(p)).collect();
    let inside = |p: P2| -> bool {
        (0..4).all(|i| {
            let (a, b) = (corners[i], corners[(i + 1) % 4]);
            (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]) >= 0.0
        })
    };
    let (mut lo, mut hi) = (0.5, 1.0);
    for _ in 0..20 {
        let mid = (lo + hi) / 2.0;
        let (cw, ch) = (w as f64 * mid / 2.0, h as f64 * mid / 2.0);
        let (cx, cy) = (w as f64 / 2.0, h as f64 / 2.0);
        let ok = [[cx - cw, cy - ch], [cx + cw, cy - ch], [cx + cw, cy + ch], [cx - cw, cy + ch]].iter().all(|&p| inside(p));
        if ok { lo = mid } else { hi = mid }
    }
    let s = lo * 0.995;
    let c = Homography([1.0 / s, 0.0, w as f64 / 2.0 - w as f64 / (2.0 * s), 0.0, 1.0 / s, h as f64 / 2.0 - h as f64 / (2.0 * s), 0.0, 0.0, 1.0]);
    hv.then(&c)
}

/// Warp image (and companion maps) to a new camera angle, up to `max_deg` about each axis,
/// keeping `keep` (sheet corners) in frame.
pub fn new_angle(img: &Rgb, extra: &[Gray], keep: &[P2], max_deg: f32, rng: &mut impl Rng) -> (Rgb, Vec<Gray>) {
    let (w, h) = (img.w, img.h);
    let a = [0, 1, 2].map(|_| (rng.random::<f64>() * 2.0 - 1.0) * max_deg as f64);
    let mut m = Homography::identity();
    for k in [1.0, 0.6, 0.35, 0.18, 0.0] {
        m = view_matrix(w, h, [a[0] * k, a[1] * k, a[2] * k]);
        let ok = keep.iter().all(|&p| {
            let q = m.apply(p);
            q[0] >= -0.005 * w as f64 && q[0] <= 1.005 * w as f64 && q[1] >= -0.005 * h as f64 && q[1] <= 1.005 * h as f64
        });
        if ok {
            break;
        }
    }
    let inv = m.inverse();
    let out = crate::geom::warp_rgb(img, &inv, w, h);
    let ex = extra.iter().map(|g| {
        let mut o = crate::geom::warp_gray(g, &inv, w, h);
        // outside the source becomes 0 in warp_gray; for transmittance maps use 1
        for (v, s) in o.d.iter_mut().zip(crate::geom::warp_gray(&Gray::new(g.w, g.h, 1.0), &inv, w, h).d) {
            if s < 0.5 {
                *v = 1.0;
            }
        }
        o
    }).collect();
    (out, ex)
}

/// Uneven light, soft occluder shadow, exposure and white balance (on linear RGB, in place).
pub fn light(lin: &mut Rgb, style: &ShotStyle, rng: &mut impl Rng) {
    let s = style.strength;
    let (w, h) = (lin.w, lin.h);
    let ang = rng.random::<f32>() * std::f32::consts::TAU;
    let grad = (0.05 + 0.17 * rng.random::<f32>()) * s * style.light as i32 as f32;
    let (cx, cy) = (rng.random::<f32>() * 0.6 - 0.3, rng.random::<f32>() * 0.6 - 0.3);
    let vig = (0.04 + 0.11 * rng.random::<f32>()) * s * style.light as i32 as f32;
    let shadow = style.shadow && rng.random::<f32>() < 0.6;
    let side = rng.random::<f32>() * std::f32::consts::TAU;
    let off = 0.25 + 0.23 * rng.random::<f32>();
    let soft = 0.04 + 0.08 * rng.random::<f32>();
    let dark = (0.15 + 0.2 * rng.random::<f32>()) * s;
    let exposure = 1.0 + (rng.random::<f32>() * 0.16 - 0.1) * s;
    let z1: f32 = rng.sample(StandardNormal);
    let z2: f32 = rng.sample(StandardNormal);
    let wb = [1.0 + 0.02 * z1 * s, 1.0, 1.0 + 0.025 * z2 * s];
    lin.d.par_chunks_mut(w * 3).enumerate().for_each(|(y, row)| {
        let yy = y as f32 / h as f32 - 0.5;
        for x in 0..w {
            let xx = x as f32 / w as f32 - 0.5;
            let mut g = 1.0 + grad * (ang.cos() * xx + ang.sin() * yy);
            g *= 1.0 - vig * ((xx - cx).powi(2) + (yy - cy).powi(2)) * 2.0;
            let mut tint = [1.0f32; 3];
            if shadow {
                let d = side.cos() * xx + side.sin() * yy - off;
                let sh = 1.0 / (1.0 + (-d / soft).exp());
                g *= 1.0 - dark * sh;
                tint = [1.0 - 0.03 * sh, 1.0 - 0.01 * sh, 1.0 + 0.02 * sh];
            }
            for c in 0..3 {
                row[x * 3 + c] *= g * exposure * wb[c] * tint[c];
            }
        }
    });
}

/// Sensor-like noise (signal dependent) + unsharp mask, on sRGB in place.
pub fn sensor(out: &mut Rgb, style: &ShotStyle, scale: f32, rng: &mut impl Rng) {
    if !style.noise {
        return;
    }
    let a = (0.0005 + 0.0015 * rng.random::<f32>()) * style.strength;
    let seed = rng.random::<u64>();
    out.d.par_chunks_mut(out.w * 3).enumerate().for_each(|(y, row)| {
        let mut r = rand_pcg::Pcg64Mcg::new((seed ^ (y as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)) as u128 | 1);
        for v in row.iter_mut() {
            let z: f32 = r.sample(StandardNormal);
            *v += z * (a * v.clamp(0.0, 1.0) + 1e-6).sqrt() * 0.5;
        }
    });
    let amount = (0.15 + 0.3 * rng.random::<f32>()) * style.strength;
    for c in 0..3 {
        let ch = out.channel(c);
        let bl = img::blur(&ch, (1.0 * scale).max(0.4));
        for (i, (v, b)) in ch.d.iter().zip(&bl.d).enumerate() {
            out.d[i * 3 + c] = v + amount * (v - b);
        }
    }
}
