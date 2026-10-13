//! Put laid-out handwriting INTO a real paper photo.
//!
//! Each glyph's real ink matte is deformed slightly (no two copies identical), given the chosen
//! pen look, and warped to its spot through the paper geometry (perspective + tracked ruled
//! lines + manual mesh). Ink acts as a filter on the photo in linear light, so paper grain,
//! printed lines and shading stay visible through it; then blur and noise are matched.

use std::collections::HashMap;

use rand::{Rng, SeedableRng};
use rand_distr::StandardNormal;
use rand_pcg::Pcg64Mcg;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::camera::{self, ShotStyle};
use crate::geom::{Homography, P2};
use crate::hand::Hand;
use crate::img::{self, Gray, Rgb};
use crate::layout::{glyph_point, Elem};
use crate::paper::{PaperGeom, PaperMeta, Placement};
use crate::pen::{self, Pen};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct RenderOpts {
    /// output scale relative to the photo (preview < 1)
    pub scale: f32,
    /// make a new random shot (angle, light, shadow, noise)
    pub random: bool,
    pub seed: u64,
    pub mess: f32,
    pub pen: Pen,
    pub shot: ShotStyle,
}

impl Default for RenderOpts {
    fn default() -> Self {
        RenderOpts { scale: 1.0, random: false, seed: 0, mess: 1.0, pen: Pen::default(), shot: ShotStyle::default() }
    }
}

fn elastic(a: &Gray, seed: u32, amount: f32) -> Gray {
    if amount <= 0.0 || a.w < 4 || a.h < 4 {
        return a.clone();
    }
    let mut rng = Pcg64Mcg::seed_from_u64(seed as u64);
    let amp = amount * 0.025 * a.w.min(a.h) as f32;
    let dx = pen::smooth_field(a.w, a.h, 3, &mut rng);
    let dy = pen::smooth_field(a.w, a.h, 3, &mut rng);
    let dens = pen::smooth_field(a.w, a.h, 3, &mut rng);
    let z: f32 = rng.sample(StandardNormal);
    let global = 1.0 + 0.06 * amount * z;
    let mut out = Gray::new(a.w, a.h, 0.0);
    for y in 0..a.h {
        for x in 0..a.w {
            let i = y * a.w + x;
            let v = a.sample(x as f32 + amp * dx[i], y as f32 + amp * dy[i]);
            out.d[i] = (v * (global + 0.05 * amount * dens[i])).clamp(0.0, 1.0);
        }
    }
    out
}

struct Patch {
    x0: i64,
    y0: i64,
    a: Gray,
}

fn warp_patch(src: &Gray, quad: &[P2; 4]) -> Option<Patch> {
    let (w, h) = (src.w as f64, src.h as f64);
    let x0 = quad.iter().map(|p| p[0]).fold(f64::MAX, f64::min).floor() as i64 - 2;
    let y0 = quad.iter().map(|p| p[1]).fold(f64::MAX, f64::min).floor() as i64 - 2;
    let x1 = quad.iter().map(|p| p[0]).fold(f64::MIN, f64::max).ceil() as i64 + 2;
    let y1 = quad.iter().map(|p| p[1]).fold(f64::MIN, f64::max).ceil() as i64 + 2;
    let (pw, ph) = ((x1 - x0) as usize, (y1 - y0) as usize);
    if pw == 0 || ph == 0 || pw * ph > 4_000_000 {
        return None;
    }
    let local: Vec<P2> = quad.iter().map(|p| [p[0] - x0 as f64, p[1] - y0 as f64]).collect();
    let m = Homography::fit(&[[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]], &local)?.inverse();
    Some(Patch { x0, y0, a: crate::geom::warp_gray(src, &m, pw, ph) })
}

/// Capsule-sweep raster of a variable-width polyline (anti-aliased by distance).
fn stroke_patch(px: &[P2], widths: &[f32], rng: &mut Pcg64Mcg) -> Option<Patch> {
    let maxw = widths.iter().cloned().fold(0.0, f32::max) as f64;
    let x0 = px.iter().map(|p| p[0]).fold(f64::MAX, f64::min).floor() as i64 - (maxw as i64 + 3);
    let y0 = px.iter().map(|p| p[1]).fold(f64::MAX, f64::min).floor() as i64 - (maxw as i64 + 3);
    let x1 = px.iter().map(|p| p[0]).fold(f64::MIN, f64::max).ceil() as i64 + (maxw as i64 + 3);
    let y1 = px.iter().map(|p| p[1]).fold(f64::MIN, f64::max).ceil() as i64 + (maxw as i64 + 3);
    let (pw, ph) = ((x1 - x0) as usize, (y1 - y0) as usize);
    if pw == 0 || ph == 0 || pw * ph > 25_000_000 {
        return None;
    }
    let mut a = Gray::new(pw, ph, 0.0);
    for k in 0..px.len().saturating_sub(1) {
        let (p, q) = ([px[k][0] - x0 as f64, px[k][1] - y0 as f64], [px[k + 1][0] - x0 as f64, px[k + 1][1] - y0 as f64]);
        let r = (widths[k].max(0.5) as f64) / 2.0;
        let (bx0, bx1) = ((p[0].min(q[0]) - r - 1.0).max(0.0) as usize, ((p[0].max(q[0]) + r + 2.0) as usize).min(pw));
        let (by0, by1) = ((p[1].min(q[1]) - r - 1.0).max(0.0) as usize, ((p[1].max(q[1]) + r + 2.0) as usize).min(ph));
        let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
        let l2 = (dx * dx + dy * dy).max(1e-9);
        for y in by0..by1 {
            for x in bx0..bx1 {
                let (fx, fy) = (x as f64 + 0.5, y as f64 + 0.5);
                let t = (((fx - p[0]) * dx + (fy - p[1]) * dy) / l2).clamp(0.0, 1.0);
                let d = ((fx - p[0] - t * dx).powi(2) + (fy - p[1] - t * dy).powi(2)).sqrt();
                let cov = (r - d + 0.5).clamp(0.0, 1.0) as f32;
                let i = y * pw + x;
                if cov > a.d[i] {
                    a.d[i] = cov;
                }
            }
        }
    }
    // ink texture: slight density variation along the stroke
    let tex = pen::smooth_field(pw, ph, (pw.max(ph) / 6).clamp(2, 40), rng);
    for (v, t) in a.d.iter_mut().zip(tex) {
        *v *= 0.88 * (1.0 - 0.15 * t.abs());
    }
    Some(Patch { x0, y0, a })
}

/// Transmittance map (1 = no ink) of the page at the photo's resolution.
pub fn ink_layer(hand: &Hand, elems: &[Elem], pg: &PaperGeom, w: usize, h: usize, opts: &RenderOpts) -> Gray {
    let gl: HashMap<&str, &crate::hand::Glyph> = hand.glyphs.iter().map(|g| (g.id.as_str(), g)).collect();
    let n = elems.len().max(1) as f32;
    let patches: Vec<Patch> = elems
        .par_iter()
        .enumerate()
        .filter_map(|(idx, e)| match e {
            Elem::G { gid, x, l, dy, s, r, sh, seed, p, .. } => {
                let g = gl.get(gid.as_str())?;
                let mut a = elastic(&hand.alpha(gid), *seed, opts.mess);
                let mut rng = Pcg64Mcg::seed_from_u64(*seed as u64 ^ 0xA5A5);
                // ink-low cycle: fades over ~120 letters, then recovers
                let fade = ((idx as f32 / n) * (n / 120.0).max(1.0)).fract();
                pen::apply(&mut a, &opts.pen, fade, &mut rng);
                if (*p - 1.0).abs() > 1e-3 {
                    a.d.iter_mut().for_each(|v| *v = (*v * p).clamp(0.0, 1.0));
                }
                let (gw, gh) = (g.w as f32, g.h as f32);
                let corners = [(0.0, 0.0), (gw, 0.0), (gw, gh), (0.0, gh)];
                let quad: Vec<P2> = corners.iter().map(|&(u, v)| {
                    let (qx, qy) = glyph_point(*x, *dy, *s, *r, *sh, g.bl, u, v);
                    pg.to_px(qx as f64, *l as f64, qy as f64)
                }).collect();
                let quad = [quad[0], quad[1], quad[2], quad[3]];
                let f = ((quad[1][0] - quad[0][0]).powi(2) + (quad[1][1] - quad[0][1]).powi(2)).sqrt() / gw.max(1.0) as f64;
                if f < 0.75 {
                    // shrinking a lot: pre-filter to avoid aliasing
                    let k = (f * 1.3).max(0.05);
                    a = a.resize(((a.w as f64 * k) as usize).max(1), ((a.h as f64 * k) as usize).max(1));
                }
                warp_patch(&a, &quad)
            }
            Elem::S { pts, w: widths, p } => {
                let px: Vec<P2> = pts.iter().map(|q| pg.to_px(q[0] as f64, q[1] as f64, q[2] as f64)).collect();
                let one = pg.to_px(pts[0][0] as f64 + 1.0, pts[0][1] as f64, pts[0][2] as f64);
                let ppm = ((one[0] - px[0][0]).powi(2) + (one[1] - px[0][1]).powi(2)).sqrt() as f32;
                let wpx: Vec<f32> = widths.iter().map(|w| w * ppm * p).collect();
                let mut rng = Pcg64Mcg::seed_from_u64(idx as u64 * 7 + 3);
                let mut patch = stroke_patch(&px, &wpx, &mut rng)?;
                let dens = opts.pen.density * if opts.pen.kind == pen::PenKind::Pencil { 0.55 } else { 1.0 };
                patch.a.d.iter_mut().for_each(|v| *v *= dens);
                Some(patch)
            }
        })
        .collect();
    let mut t = Gray::new(w, h, 1.0);
    for p in patches {
        for y in 0..p.a.h {
            let yy = p.y0 + y as i64;
            if yy < 0 || yy >= h as i64 {
                continue;
            }
            for x in 0..p.a.w {
                let xx = p.x0 + x as i64;
                if xx < 0 || xx >= w as i64 {
                    continue;
                }
                let v = p.a.d[y * p.a.w + x];
                if v > 0.0 {
                    t.d[yy as usize * w + xx as usize] *= 1.0 - v.clamp(0.0, 1.0);
                }
            }
        }
    }
    t
}

/// Blur that changes over the page: `pts` = (x, y, sigma) in px, spread between them by
/// inverse distance, made from three blurs of the whole layer.
fn varying_blur(t: &Gray, pts: &[[f64; 3]]) -> Gray {
    let lo = pts.iter().map(|q| q[2]).fold(f64::MAX, f64::min);
    let hi = pts.iter().map(|q| q[2]).fold(0.0, f64::max);
    let levels = [lo, (lo + hi) / 2.0, hi];
    let blurred: Vec<Gray> = levels.par_iter().map(|&s| if s > 0.25 { img::blur(t, s as f32) } else { t.clone() }).collect();
    // sigma on a coarse grid, then read back per pixel
    const CELL: usize = 16;
    let (gw, gh) = (t.w / CELL + 2, t.h / CELL + 2);
    let grid: Vec<f64> = (0..gw * gh)
        .map(|i| {
            let (x, y) = (((i % gw) * CELL) as f64, ((i / gw) * CELL) as f64);
            let (mut a, mut b) = (0.0, 0.0);
            for q in pts {
                let w = 1.0 / ((x - q[0]).powi(2) + (y - q[1]).powi(2) + 1.0);
                a += w * q[2];
                b += w;
            }
            a / b
        })
        .collect();
    let mut out = Gray::new(t.w, t.h, 1.0);
    out.d.par_chunks_mut(t.w).enumerate().for_each(|(y, row)| {
        let gy = y as f64 / CELL as f64;
        let (j, fy) = (gy as usize, gy.fract());
        for (x, v) in row.iter_mut().enumerate() {
            let gx = x as f64 / CELL as f64;
            let (i, fx) = (gx as usize, gx.fract());
            let g = |ii: usize, jj: usize| grid[(jj.min(gh - 1)) * gw + ii.min(gw - 1)];
            let s = (g(i, j) * (1.0 - fx) + g(i + 1, j) * fx) * (1.0 - fy) + (g(i, j + 1) * (1.0 - fx) + g(i + 1, j + 1) * fx) * fy;
            let k = y * t.w + x;
            *v = if s <= levels[1] {
                let f = ((s - levels[0]) / (levels[1] - levels[0]).max(1e-6)).clamp(0.0, 1.0) as f32;
                blurred[0].d[k] * (1.0 - f) + blurred[1].d[k] * f
            } else {
                let f = ((s - levels[1]) / (levels[2] - levels[1]).max(1e-6)).clamp(0.0, 1.0) as f32;
                blurred[1].d[k] * (1.0 - f) + blurred[2].d[k] * f
            };
        }
    });
    out
}

/// Corners (px) of the box around the ink of a transmittance layer, `margin` px wider.
fn ink_box(t: &Gray, margin: f64) -> Vec<P2> {
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
    for y in (0..t.h).step_by(2) {
        for x in (0..t.w).step_by(2) {
            if t.d[y * t.w + x] < 0.8 {
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
            }
        }
    }
    if x0 > x1 {
        return vec![];
    }
    let (a, b, c, d) = (x0 as f64 - margin, y0 as f64 - margin, x1 as f64 + margin, y1 as f64 + margin);
    vec![[a, b], [c, b], [c, d], [a, d]]
}

/// Render one page. `photo` is the shot's photo already scaled by `opts.scale` (sRGB 0..1).
pub fn render(paper: &PaperMeta, photo: &Rgb, hand: &Hand, elems: &[Elem], place: &Placement, opts: &RenderOpts) -> Rgb {
    let scale = opts.scale as f64;
    let pg = PaperGeom::new(paper, place, scale);
    let shot = &paper.shots[place.shot.unwrap_or(0).min(paper.shots.len() - 1)];
    let mut t = ink_layer(hand, elems, &pg, photo.w, photo.h, opts);
    // match the photo's own blur. Glyph mattes already carry the blur of the photo they were
    // cut from (about 1 px there), enlarged or shrunk with them: only add what is missing.
    let blur = shot.blur_px * opts.scale;
    let ks: Vec<f32> = elems
        .iter()
        .filter_map(|e| match e {
            Elem::G { x, l, dy, s, .. } => {
                let a = pg.to_px(*x as f64, *l as f64, *dy as f64);
                let b = pg.to_px(*x as f64 + 1.0, *l as f64, *dy as f64);
                Some(s * ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt() as f32)
            }
            _ => None,
        })
        .collect();
    let own = if ks.is_empty() { 0.5 * opts.scale } else { crate::signal::median(&ks) };
    let add = (blur * blur - own * own).max(0.0).sqrt();
    // where the photo is blurrier (out of the focus), the writing is too
    let map: Vec<[f64; 3]> = shot
        .blur_map
        .iter()
        .map(|q| [q[0] * scale, q[1] * scale, ((q[2] as f32 * opts.scale).powi(2) - own * own).max(0.0).sqrt() as f64])
        .collect();
    let (lo, hi) = map.iter().fold((f64::MAX, 0.0f64), |a, q| (a.0.min(q[2]), a.1.max(q[2])));
    if map.len() >= 6 && hi - lo > 0.3 {
        t = varying_blur(&t, &map);
    } else if add > 0.25 {
        t = img::blur(&t, add);
    }
    let ink = opts.pen.color.unwrap_or(hand.meta.ink);
    let ink_lin = ink.map(|c| c.clamp(0.01, 1.0).powf(2.2));
    let mut lin = img::to_linear(photo);
    lin.d.par_chunks_mut(3).zip(t.d.par_iter()).for_each(|(px, tv)| {
        let cov = 1.0 - tv;
        for c in 0..3 {
            px[c] *= 1.0 - cov * (1.0 - ink_lin[c]);
        }
    });
    let mean_ink = (ink_lin[0] + ink_lin[1] + ink_lin[2]) / 3.0;
    let mut trans = Gray { w: t.w, h: t.h, d: t.d.par_iter().map(|tv| 1.0 - (1.0 - tv) * (1.0 - mean_ink)).collect() };
    let mut rng = Pcg64Mcg::seed_from_u64(opts.seed);
    if opts.random {
        camera::light(&mut lin, &opts.shot, &mut rng);
        if opts.shot.angle && opts.shot.angle_max > 0.0 && opts.shot.strength > 0.0 {
            // the sheet when its corners were found, and always the writing: a new angle never cuts it off
            let mut keep: Vec<P2> = if shot.found { shot.corners.iter().map(|c| [c[0] * scale, c[1] * scale]).collect() } else { vec![[photo.w as f64 / 2.0, photo.h as f64 / 2.0]] };
            keep.extend(ink_box(&trans, 3.0 * opts.scale as f64 + 0.006 * trans.w.max(trans.h) as f64));
            let (l2, ex) = camera::new_angle(&lin, &[trans], &keep, opts.shot.angle_max, &mut rng);
            lin = l2;
            trans = ex.into_iter().next().unwrap();
        }
    }
    let mut out = img::to_srgb(&lin);
    // the multiply darkened the photo's own noise under the ink; put the missing part back
    let sigma = shot.noise;
    let nseed = rng.random::<u64>();
    let nblur = 0.6 * opts.scale.max(0.5);
    let noise = {
        let mut r = Pcg64Mcg::seed_from_u64(nseed);
        let g = Gray { w: out.w, h: out.h, d: (0..out.w * out.h).map(|_| r.sample::<f32, _>(StandardNormal)).collect() };
        img::blur(&g, nblur)
    };
    out.d.par_chunks_mut(3).enumerate().for_each(|(i, px)| {
        let miss = (1.0 - trans.d[i] * trans.d[i]).clamp(0.0, 1.0).sqrt();
        let n = noise.d[i] * 1.6 * sigma * miss;
        for v in px.iter_mut() {
            *v += n;
        }
    });
    if opts.random {
        camera::sensor(&mut out, &opts.shot, opts.scale, &mut rng);
    }
    out.d.par_iter_mut().for_each(|v| *v = v.clamp(0.0, 1.0));
    out
}
