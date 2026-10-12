//! Synthetic test data: a sample sheet "filled in" with a script font, notebook paper, and
//! fake phone photos of both.

#![allow(dead_code)]

use ab_glyph::{Font, FontArc, PxScale, ScaleFont};
use hw_core::geom::Homography;
use hw_core::img::{Gray, Rgb};
use hw_core::sheet::{self, Kind};
use rand::{Rng, SeedableRng};
use rand_distr::StandardNormal;
use rand_pcg::Pcg64Mcg;

pub fn font(name: &str) -> Option<FontArc> {
    let data = std::fs::read(format!(r"C:\Windows\Fonts\{name}")).ok()?;
    FontArc::try_from_vec(data).ok()
}

/// Draw text with per-letter jitter into an alpha layer; baseline at (x, y) px.
pub fn draw_text(layer: &mut Gray, x: f32, y: f32, text: &str, px: f32, rng: &mut Pcg64Mcg, hand: &FontArc, fallback: &FontArc) {
    let mut cx = x;
    for ch in text.chars() {
        let f = if hand.glyph_id(ch).0 != 0 { hand } else { fallback };
        let scaled = f.as_scaled(PxScale::from(px));
        let id = f.glyph_id(ch);
        let dy: f32 = rng.sample::<f32, _>(StandardNormal) * 0.015 * px;
        let g = id.with_scale_and_position(PxScale::from(px), ab_glyph::point(cx, y + dy));
        if let Some(o) = f.outline_glyph(g) {
            let b = o.px_bounds();
            o.draw(|gx, gy, c| {
                let (xx, yy) = (b.min.x as i32 + gx as i32, b.min.y as i32 + gy as i32);
                if xx >= 0 && yy >= 0 && (xx as usize) < layer.w && (yy as usize) < layer.h {
                    let i = yy as usize * layer.w + xx as usize;
                    layer.d[i] = layer.d[i].max(c);
                }
            });
        }
        cx += scaled.h_advance(id) * rng.random_range(0.95..1.08);
    }
}

fn rect(img: &mut Rgb, x0: f64, y0: f64, x1: f64, y1: f64, c: [f32; 3], ppm: f64, outline: bool) {
    let (a, b, cc, d) = ((x0 * ppm) as usize, (y0 * ppm) as usize, (x1 * ppm) as usize, (y1 * ppm) as usize);
    for y in b..=d.min(img.h - 1) {
        for x in a..=cc.min(img.w - 1) {
            if !outline || x == a || x == cc || y == b || y == d {
                let i = (y * img.w + x) * 3;
                img.d[i..i + 3].copy_from_slice(&c);
            }
        }
    }
}

/// One sheet page filled with "handwriting" (blue ink), flat, at ppm.
pub fn fill_page(page: &sheet::Page, ppm: f64, seed: u64) -> Rgb {
    let hand = font("inkfree.ttf").expect("Ink Free font");
    let fb = font("arial.ttf").expect("Arial");
    let mut rng = Pcg64Mcg::seed_from_u64(seed + page.index as u64 * 31 + page.code[0] as u64);
    let (w, h) = ((sheet::PAGE_W * ppm) as usize, (sheet::PAGE_H * ppm) as usize);
    let m = sheet::marker_raster(page.code, ppm);
    let mut img = Rgb { w, h, d: m.d.iter().flat_map(|&v| [v, v, v]).collect() };
    let guide = [1.0, 0.86, 0.80];
    let mut layer = Gray::new(w, h, 0.0);
    for it in &page.items {
        let [x, y, bw, bh] = it.r;
        if it.kind == Kind::Glyph {
            rect(&mut img, x, y, x + bw, y + bh, guide, ppm, true);
        }
        rect(&mut img, x, it.bl, x + bw, it.bl, guide, ppm, false);
        match it.kind {
            Kind::Glyph => {
                let px = (7.5 * ppm) as f32 * rng.random_range(0.92..1.08);
                let ox = rng.random_range(1.5..3.0) * ppm;
                draw_text(&mut layer, ((x * ppm) + ox) as f32, (it.bl * ppm) as f32, &it.text, px, &mut rng, &hand, &fb);
            }
            Kind::Line => draw_text(&mut layer, ((x + 1.0) * ppm) as f32, (it.bl * ppm) as f32, &it.text, (6.0 * ppm) as f32, &mut rng, &hand, &fb),
        }
    }
    let ink = [0.12, 0.16, 0.45];
    for i in 0..w * h {
        let a = layer.d[i];
        for c in 0..3 {
            img.d[i * 3 + c] *= 1.0 - a * (1.0 - ink[c]);
        }
    }
    img
}

/// Notebook paper (lined or grid), flat, at ppm.
pub fn lined_paper(ppm: f64, size: (f64, f64), pitch: f64, grid: bool) -> Rgb {
    let (w, h) = ((size.0 * ppm) as usize, (size.1 * ppm) as usize);
    let mut rng = Pcg64Mcg::seed_from_u64(1);
    let mut img = Rgb { w, h, d: (0..w * h).flat_map(|_| {
        let n: f32 = rng.sample::<f32, _>(StandardNormal) * 0.01;
        [0.97 + n, 0.97 + n, 0.95 + n]
    }).collect() };
    let blue = [0.55, 0.70, 0.90];
    let step = if grid { 5.0 } else { pitch };
    let lw = (0.2 * ppm).max(1.0);
    let mut y = 15.0;
    while y < size.1 - 3.0 {
        rect(&mut img, 0.0, y, size.0 - 0.1, y + lw / ppm, blue, ppm, false);
        y += step;
    }
    if grid {
        let mut x = 3.0;
        while x < size.0 {
            rect(&mut img, x, 0.0, x + lw / ppm, size.1 - 0.1, blue, ppm, false);
            x += step;
        }
    } else {
        rect(&mut img, 25.0, 0.0, 25.0 + 0.25, size.1 - 0.1, [0.9, 0.45, 0.45], ppm, false);
    }
    img
}

/// Put a flat page on a desk with perspective, optional curl, uneven light, blur, noise, JPEG.
pub fn photo(page: &Rgb, seed: u64, out_w: usize, curl: f32) -> Rgb {
    let mut rng = Pcg64Mcg::seed_from_u64(seed);
    let out_h = out_w * 4 / 3;
    let (pw, ph) = (page.w as f64, page.h as f64);
    let s = 0.8 * (out_w as f64 / pw).min(out_h as f64 / ph);
    let (cx, cy) = (out_w as f64 / 2.0, out_h as f64 / 2.0);
    let mut dst = [[cx - pw * s / 2.0, cy - ph * s / 2.0], [cx + pw * s / 2.0, cy - ph * s / 2.0], [cx + pw * s / 2.0, cy + ph * s / 2.0], [cx - pw * s / 2.0, cy + ph * s / 2.0]];
    for d in dst.iter_mut() {
        d[0] += rng.random_range(-0.04..0.04) * pw * s;
        d[1] += rng.random_range(-0.04..0.04) * ph * s;
    }
    let hm = Homography::fit(&[[0.0, 0.0], [pw, 0.0], [pw, ph], [0.0, ph]], &dst).unwrap();
    let inv = hm.inverse();
    let base = [0.42, 0.30, 0.20].map(|v| v * rng.random_range(0.7..1.2));
    let ang = rng.random_range(0.0..std::f64::consts::TAU);
    let mut d = vec![0f32; out_w * out_h * 3];
    let mut px = [0f32; 3];
    for y in 0..out_h {
        for x in 0..out_w {
            let mut p = inv.apply([x as f64, y as f64]);
            if curl > 0.0 {
                p[1] -= curl as f64 * ph * 0.02 * (std::f64::consts::PI * p[0] / pw).sin();
            }
            let i = (y * out_w + x) * 3;
            let inside = p[0] >= 0.0 && p[1] >= 0.0 && p[0] < pw - 1.0 && p[1] < ph - 1.0;
            if inside {
                page.sample(p[0] as f32, p[1] as f32, &mut px);
            } else {
                let grain = 0.04 * ((x as f64 / out_w as f64 * 40.0) + 3.0 * (y as f64 / out_h as f64 * 7.0).sin()).sin();
                px = base.map(|b| (b * (1.0 + grain)) as f32);
            }
            let g = 1.0 + 0.18 * (ang.cos() * (x as f64 / out_w as f64 - 0.5) + ang.sin() * (y as f64 / out_h as f64 - 0.5));
            let tint = [1.0, 0.98, 0.93];
            for c in 0..3 {
                d[i + c] = px[c] * (g as f32) * tint[c];
            }
        }
    }
    let mut img = Rgb { w: out_w, h: out_h, d };
    for c in 0..3 {
        let ch = hw_core::img::blur(&img.channel(c), 0.9);
        for (i, v) in ch.d.iter().enumerate() {
            let n: f32 = rng.sample::<f32, _>(StandardNormal) * 0.012;
            img.d[i * 3 + c] = (v + n).clamp(0.0, 1.0);
        }
    }
    let jpg = hw_core::io::rgb_jpeg(&img, 88);
    Rgb::from_image(&image::load_from_memory(&jpg).unwrap().to_rgb8())
}

pub fn scratch() -> std::path::PathBuf {
    let p = std::env::temp_dir().join("hw-core-tests");
    std::fs::create_dir_all(&p).unwrap();
    p
}
