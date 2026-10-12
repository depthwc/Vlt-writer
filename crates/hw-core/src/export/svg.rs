//! SVG of a laid-out page. Each glyph's real ink is embedded once (<image> in <defs>) and placed
//! with <use> transforms, so files stay small and letters stay editable. Layer "plot" holds
//! centre-line pen paths for pen plotters (hidden by default).

use std::collections::{BTreeSet, HashMap};
use std::fmt::Write;

use base64::Engine;

use crate::hand::Hand;
use crate::layout::{glyph_point, Elem, Geometry};

pub const PAGE_W: f32 = 210.0;
pub const PAGE_H: f32 = 297.0;
pub const MARGIN_X: f32 = 22.0;
pub const TOP: f32 = 28.0;

pub fn line_y(l: f32, pitch: f32) -> f32 {
    TOP + l * pitch
}

fn glyph_png(hand: &Hand, gid: &str, ink: [f32; 3]) -> String {
    let a = hand.alpha(gid);
    let col = ink.map(|c| (c.clamp(0.0, 1.0) * 255.0) as u8);
    let mut img = image::RgbaImage::new(a.w as u32, a.h as u32);
    for (i, p) in img.pixels_mut().enumerate() {
        *p = image::Rgba([col[0], col[1], col[2], (a.d[i].clamp(0.0, 1.0) * 255.0) as u8]);
    }
    let mut buf = std::io::Cursor::new(Vec::new());
    let _ = img.write_to(&mut buf, image::ImageFormat::Png);
    base64::engine::general_purpose::STANDARD.encode(buf.into_inner())
}

/// Centre-line polylines of the page in page millimetres (for plotters).
pub fn plot_paths(hand: &Hand, elems: &[Elem], geo: &Geometry) -> Vec<Vec<[f32; 2]>> {
    let gl: HashMap<&str, &crate::hand::Glyph> = hand.glyphs.iter().map(|g| (g.id.as_str(), g)).collect();
    let mut out = vec![];
    for e in elems {
        match e {
            Elem::G { gid, x, l, dy, s, r, sh, .. } => {
                if let Some(g) = gl.get(gid.as_str()) {
                    for line in &g.skel {
                        out.push(line.iter().map(|[u, v]| {
                            let (px, py) = glyph_point(*x, *dy, *s, *r, *sh, g.bl, *u, *v);
                            [MARGIN_X + px, line_y(*l, geo.pitch_mm) + py]
                        }).collect());
                    }
                }
            }
            Elem::S { pts, .. } => out.push(pts.iter().map(|[x, l, dy]| [MARGIN_X + x, line_y(*l, geo.pitch_mm) + dy]).collect()),
        }
    }
    out
}

pub fn page_svg(hand: &Hand, elems: &[Elem], geo: &Geometry, ink: [f32; 3], plot: bool) -> String {
    let gl: HashMap<&str, &crate::hand::Glyph> = hand.glyphs.iter().map(|g| (g.id.as_str(), g)).collect();
    let col = format!("#{:02x}{:02x}{:02x}", (ink[0] * 255.0) as u8, (ink[1] * 255.0) as u8, (ink[2] * 255.0) as u8);
    let used: BTreeSet<&str> = elems.iter().filter_map(|e| if let Elem::G { gid, .. } = e { Some(gid.as_str()) } else { None }).filter(|g| gl.contains_key(g)).collect();
    let mut s = String::new();
    let _ = write!(
        s,
        r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{PAGE_W}mm" height="{PAGE_H}mm" viewBox="0 0 {PAGE_W} {PAGE_H}"><defs>"#
    );
    for gid in &used {
        let g = gl[gid];
        let _ = write!(s, r#"<image id="g{gid}" width="{}" height="{}" xlink:href="data:image/png;base64,{}"/>"#, g.w, g.h, glyph_png(hand, gid, ink));
    }
    s.push_str(r##"</defs><rect width="100%" height="100%" fill="#fff"/><g id="ink">"##);
    for e in elems {
        match e {
            Elem::G { gid, x, l, dy, s: sc, r, sh, wi, ci, .. } => {
                let Some(g) = gl.get(gid.as_str()) else { continue };
                let (tx, ty) = (MARGIN_X + x, line_y(*l, geo.pitch_mm) + dy);
                let _ = write!(
                    s,
                    r##"<use xlink:href="#g{gid}" data-w="{wi}" data-c="{ci}" transform="translate({tx:.3},{ty:.3}) rotate({r:.2}) matrix(1,0,{:.4},1,0,0) scale({sc:.5}) translate(0,{:.2})"/>"##,
                    -sh,
                    -g.bl
                );
            }
            Elem::S { pts, w, .. } => {
                let d: Vec<String> = pts.iter().map(|[x, l, dy]| format!("{:.2},{:.2}", MARGIN_X + x, line_y(*l, geo.pitch_mm) + dy)).collect();
                let mw = w.iter().sum::<f32>() / w.len().max(1) as f32;
                let _ = write!(s, r#"<path d="M{}" fill="none" stroke="{col}" stroke-width="{mw:.3}" stroke-linecap="round" stroke-linejoin="round" opacity="0.9"/>"#, d.join(" L"));
            }
        }
    }
    s.push_str("</g>");
    if plot {
        s.push_str(r##"<g id="plot" style="display:none" fill="none" stroke="#000" stroke-width="0.3">"##);
        for p in plot_paths(hand, elems, geo) {
            let pts: Vec<String> = p.iter().map(|q| format!("{:.2},{:.2}", q[0], q[1])).collect();
            let _ = write!(s, r#"<polyline points="{}"/>"#, pts.join(" "));
        }
        s.push_str("</g>");
    }
    s.push_str("</svg>");
    s
}
