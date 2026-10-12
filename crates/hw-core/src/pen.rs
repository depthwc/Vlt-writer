//! Pen models applied to real ink mattes: the person's own pen, or ballpoint / gel / pencil /
//! fountain / felt-tip looks, plus "ink running low".

use serde::{Deserialize, Serialize};

use crate::img::Gray;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum PenKind {
    /// exactly as photographed
    #[default]
    Mine,
    Ballpoint,
    Gel,
    Pencil,
    Fountain,
    Felt,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Pen {
    pub kind: PenKind,
    /// ink colour as reflectance relative to paper; None = the hand's measured ink
    pub color: Option<[f32; 3]>,
    /// overall darkness multiplier
    pub density: f32,
    /// 0..1: ink fades over a stretch of writing, then recovers (refill / new pen angle)
    pub ink_low: f32,
}

impl Default for Pen {
    fn default() -> Self {
        Pen { kind: PenKind::Mine, color: None, density: 1.0, ink_low: 0.0 }
    }
}

pub const PRESETS: &[(&str, [f32; 3])] = &[
    ("blue", [0.17, 0.22, 0.52]),
    ("black", [0.14, 0.14, 0.16]),
    ("violet", [0.30, 0.22, 0.50]),
    ("red", [0.72, 0.20, 0.20]),
    ("green", [0.20, 0.42, 0.28]),
    ("pencil", [0.40, 0.40, 0.42]),
];

/// Smooth random field in [-1, 1] (bilinear upsample of a coarse grid).
pub fn smooth_field(w: usize, h: usize, cells: usize, rng: &mut impl rand::Rng) -> Vec<f32> {
    let g = cells.max(2);
    let grid: Vec<f32> = (0..(g + 1) * (g + 1)).map(|_| rng.random::<f32>() * 2.0 - 1.0).collect();
    let mut out = vec![0f32; w * h];
    for y in 0..h {
        let fy = y as f32 / h.max(1) as f32 * g as f32;
        let (iy, ty) = ((fy as usize).min(g - 1), fy - (fy as usize).min(g - 1) as f32);
        for x in 0..w {
            let fx = x as f32 / w.max(1) as f32 * g as f32;
            let (ix, tx) = ((fx as usize).min(g - 1), fx - (fx as usize).min(g - 1) as f32);
            let a = grid[iy * (g + 1) + ix] * (1.0 - tx) + grid[iy * (g + 1) + ix + 1] * tx;
            let b = grid[(iy + 1) * (g + 1) + ix] * (1.0 - tx) + grid[(iy + 1) * (g + 1) + ix + 1] * tx;
            out[y * w + x] = a * (1.0 - ty) + b * ty;
        }
    }
    out
}

/// Change a matte to look like it was written with `pen`. `fade` in [0,1] from ink-low cycle.
pub fn apply(a: &mut Gray, pen: &Pen, fade: f32, rng: &mut impl rand::Rng) {
    let (w, h) = (a.w, a.h);
    let dens = pen.density * (1.0 - 0.45 * pen.ink_low * fade);
    match pen.kind {
        PenKind::Mine => {
            if (dens - 1.0).abs() > 1e-3 {
                a.d.iter_mut().for_each(|v| *v = (*v * dens).clamp(0.0, 1.0));
            }
        }
        PenKind::Ballpoint => {
            // streaky density along strokes + rare skips
            let streak = smooth_field(w, h, 6, rng);
            let skip = smooth_field(w, h, 10, rng);
            for i in 0..w * h {
                let mut v = a.d[i].powf(0.9) * (0.92 + 0.1 * streak[i]) * dens;
                if skip[i] > 0.82 {
                    v *= 0.45;
                }
                a.d[i] = v.clamp(0.0, 1.0);
            }
        }
        PenKind::Gel => {
            for v in a.d.iter_mut() {
                *v = ((*v * 1.35).clamp(0.0, 1.0).powf(0.7) * dens).clamp(0.0, 1.0);
            }
        }
        PenKind::Pencil => {
            // graphite: lighter, grainy, catches the paper tooth
            for v in a.d.iter_mut() {
                let grain = rng.random::<f32>();
                *v = (*v * (0.35 + 0.45 * grain) * dens).clamp(0.0, 0.85);
            }
        }
        PenKind::Fountain => {
            let pool = smooth_field(w, h, 4, rng);
            for i in 0..w * h {
                a.d[i] = (a.d[i].powf(0.8) * (0.95 + 0.15 * pool[i]) * dens).clamp(0.0, 1.0);
            }
        }
        PenKind::Felt => {
            let src = a.clone();
            for y in 0..h {
                for x in 0..w {
                    let mut m = 0f32;
                    for (dx, dy) in [(0i32, 0i32), (1, 0), (-1, 0), (0, 1), (0, -1)] {
                        let (xx, yy) = (x as i32 + dx, y as i32 + dy);
                        if xx >= 0 && yy >= 0 && (xx as usize) < w && (yy as usize) < h {
                            m = m.max(src.at(xx as usize, yy as usize));
                        }
                    }
                    a.set(x, y, ((m * 1.2).clamp(0.0, 1.0) * dens).clamp(0.0, 1.0));
                }
            }
        }
    }
}
