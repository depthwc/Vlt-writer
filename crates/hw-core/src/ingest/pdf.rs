//! PDF via PDFium: characters with boxes and sizes -> lines -> paragraphs/headings/lists;
//! vector line segments -> tables (grids of ruled lines) and horizontal rules.

use std::path::Path;
use std::sync::OnceLock;

use parking_lot::Mutex;
use pdfium_render::prelude::*;

use super::{Block, END, SUB, SUP};
use crate::charset;

static PDFIUM: OnceLock<Mutex<Pdfium>> = OnceLock::new();

fn pdfium(dir: Option<&Path>) -> anyhow::Result<&'static Mutex<Pdfium>> {
    if let Some(p) = PDFIUM.get() {
        return Ok(p);
    }
    let bindings = match dir {
        Some(d) => Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(d)),
        None => Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path("./")).or_else(|_| Pdfium::bind_to_system_library()),
    }
    .map_err(|e| anyhow::anyhow!("PDF engine (pdfium.dll) not found: {e:?}"))?;
    let _ = PDFIUM.set(Mutex::new(Pdfium::new(bindings)));
    Ok(PDFIUM.get().unwrap())
}

#[derive(Clone, Debug)]
struct Ch {
    c: char,
    x0: f32,
    x1: f32,
    y0: f32, // top (y down)
    y1: f32, // bottom
    size: f32,
}

#[derive(Clone, Copy, Debug)]
struct Seg {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
}

impl Seg {
    fn horizontal(&self) -> bool {
        (self.y1 - self.y0).abs() < 1.5 && (self.x1 - self.x0).abs() > 3.0
    }
    fn vertical(&self) -> bool {
        (self.x1 - self.x0).abs() < 1.5 && (self.y1 - self.y0).abs() > 3.0
    }
}

pub fn from_pdf(data: &[u8], pdfium_dir: Option<&Path>) -> anyhow::Result<Vec<Block>> {
    let lib = pdfium(pdfium_dir)?.lock();
    let doc = lib.load_pdf_from_byte_slice(data, None).map_err(|e| anyhow::anyhow!("cannot open PDF: {e:?}"))?;
    let mut blocks = vec![];
    for page in doc.pages().iter() {
        let ph = page.height().value;
        let pw = page.width().value;
        let mut chars = vec![];
        if let Ok(text) = page.text() {
            for c in text.chars().iter() {
                let Some(u) = c.unicode_char() else { continue };
                if u == '\r' || u == '\n' {
                    continue;
                }
                let Ok(b) = c.loose_bounds() else { continue };
                chars.push(Ch {
                    c: u,
                    x0: b.left().value,
                    x1: b.right().value,
                    y0: ph - b.top().value,
                    y1: ph - b.bottom().value,
                    size: c.scaled_font_size().value,
                });
            }
        }
        let mut segs = vec![];
        for obj in page.objects().iter() {
            let Some(path) = obj.as_path_object() else { continue };
            let Ok(m) = path.matrix() else { continue };
            let pts: Vec<(PdfPathSegmentType, f32, f32)> =
                path.segments().transform(m).iter().map(|s| (s.segment_type(), s.x().value, ph - s.y().value)).collect();
            let mut last: Option<(f32, f32)> = None;
            let mut start: Option<(f32, f32)> = None;
            let mut sub = vec![];
            for (t, x, y) in pts {
                match t {
                    PdfPathSegmentType::MoveTo => {
                        last = Some((x, y));
                        start = Some((x, y));
                    }
                    PdfPathSegmentType::LineTo => {
                        if let Some((lx, ly)) = last {
                            sub.push(Seg { x0: lx.min(x), y0: ly.min(y), x1: lx.max(x), y1: ly.max(y) });
                        }
                        last = Some((x, y));
                    }
                    _ => last = Some((x, y)),
                }
            }
            let _ = start;
            // A thin filled rectangle is a line too.
            if !sub.is_empty() {
                let (x0, x1) = (sub.iter().map(|s| s.x0).fold(f32::MAX, f32::min), sub.iter().map(|s| s.x1).fold(f32::MIN, f32::max));
                let (y0, y1) = (sub.iter().map(|s| s.y0).fold(f32::MAX, f32::min), sub.iter().map(|s| s.y1).fold(f32::MIN, f32::max));
                if y1 - y0 < 2.5 && x1 - x0 > 3.0 {
                    segs.push(Seg { x0, y0: (y0 + y1) / 2.0, x1, y1: (y0 + y1) / 2.0 });
                    continue;
                }
                if x1 - x0 < 2.5 && y1 - y0 > 3.0 {
                    segs.push(Seg { x0: (x0 + x1) / 2.0, y0, x1: (x0 + x1) / 2.0, y1 });
                    continue;
                }
            }
            segs.extend(sub.into_iter().filter(|s| s.horizontal() || s.vertical()));
        }
        blocks.extend(page_blocks(chars, segs, pw));
    }
    Ok(blocks)
}

fn page_blocks(chars: Vec<Ch>, segs: Vec<Seg>, pw: f32) -> Vec<Block> {
    // ---- tables: clusters of crossing horizontal + vertical segments
    let n = segs.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(p: &mut [usize], mut x: usize) -> usize {
        while p[x] != x {
            p[x] = p[p[x]];
            x = p[x];
        }
        x
    }
    for i in 0..n {
        for j in i + 1..n {
            let (a, b) = (segs[i], segs[j]);
            let (h, v) = if a.horizontal() && b.vertical() { (a, b) } else if b.horizontal() && a.vertical() { (b, a) } else { continue };
            if v.x0 >= h.x0 - 2.0 && v.x0 <= h.x1 + 2.0 && h.y0 >= v.y0 - 2.0 && h.y0 <= v.y1 + 2.0 {
                let (ra, rb) = (find(&mut parent, i), find(&mut parent, j));
                parent[ra] = rb;
            }
        }
    }
    let mut clusters: std::collections::HashMap<usize, Vec<usize>> = Default::default();
    for i in 0..n {
        let r = find(&mut parent, i);
        clusters.entry(r).or_default().push(i);
    }
    let mut items: Vec<(f32, Block)> = vec![];
    let mut boxes: Vec<[f32; 4]> = vec![];
    let mut used = vec![false; chars.len()];
    let mut in_table = vec![false; n];
    for ids in clusters.values() {
        let hs: Vec<Seg> = ids.iter().map(|&i| segs[i]).filter(|s| s.horizontal()).collect();
        let vs: Vec<Seg> = ids.iter().map(|&i| segs[i]).filter(|s| s.vertical()).collect();
        if hs.len() < 2 || vs.len() < 2 {
            continue;
        }
        for &i in ids {
            in_table[i] = true;
        }
        let uniq = |mut v: Vec<f32>| -> Vec<f32> {
            v.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let mut o: Vec<f32> = vec![];
            for x in v {
                if o.last().is_none_or(|&l| x - l > 2.0) {
                    o.push(x);
                }
            }
            o
        };
        let rows = uniq(hs.iter().map(|s| s.y0).collect());
        let cols = uniq(vs.iter().map(|s| s.x0).collect());
        if rows.len() < 2 || cols.len() < 2 {
            continue;
        }
        let bx = [cols[0], rows[0], cols[cols.len() - 1], rows[rows.len() - 1]];
        let mut cells = vec![vec![Vec::<Ch>::new(); cols.len() - 1]; rows.len() - 1];
        for (k, ch) in chars.iter().enumerate() {
            let (cx, cy) = ((ch.x0 + ch.x1) / 2.0, (ch.y0 + ch.y1) / 2.0);
            if cx < bx[0] || cx > bx[2] || cy < bx[1] || cy > bx[3] {
                continue;
            }
            used[k] = true;
            let r = rows.windows(2).position(|w| cy >= w[0] && cy < w[1]);
            let c = cols.windows(2).position(|w| cx >= w[0] && cx < w[1]);
            if let (Some(r), Some(c)) = (r, c) {
                cells[r][c].push(ch.clone());
            }
        }
        let table: Vec<Vec<String>> = cells
            .into_iter()
            .map(|row| row.into_iter().map(|cell| lines(cell).into_iter().map(|l| l.text.trim().to_string()).collect::<Vec<_>>().join(" ")).collect())
            .collect();
        boxes.push(bx);
        items.push((bx[1], Block::Table { rows: table }));
    }
    // ---- horizontal rules outside tables
    let mut rules: Vec<f32> = segs
        .iter()
        .enumerate()
        .filter(|(i, s)| !in_table[*i] && s.horizontal() && s.x1 - s.x0 > 0.3 * pw)
        .map(|(_, s)| s.y0)
        .filter(|y| !boxes.iter().any(|b| *y >= b[1] - 2.0 && *y <= b[3] + 2.0))
        .collect();
    rules.sort_by(|a, b| a.partial_cmp(b).unwrap());
    rules.dedup_by(|a, b| (*a - *b).abs() < 4.0);
    items.extend(rules.into_iter().map(|y| (y, Block::Rule)));
    // ---- text lines -> paragraphs
    let free: Vec<Ch> = chars.into_iter().enumerate().filter(|(k, _)| !used[*k]).map(|(_, c)| c).collect();
    let ls = lines(free);
    let sizes: Vec<f32> = ls.iter().map(|l| l.size).collect();
    let med = crate::signal::median(&sizes).max(1.0);
    let mut i = 0;
    while i < ls.len() {
        let l = &ls[i];
        let text = charset::normalize(l.text.trim());
        if text.is_empty() {
            i += 1;
            continue;
        }
        if l.size > 1.2 * med && text.chars().count() < 120 {
            items.push((l.y0, Block::H { text, level: if l.size > 1.5 * med { 1 } else { 2 } }));
            i += 1;
            continue;
        }
        // gather following lines of the same paragraph
        let mut para = text;
        let y0 = l.y0;
        let mut j = i + 1;
        while j < ls.len() {
            let (prev, nx) = (&ls[j - 1], &ls[j]);
            let gap = nx.y0 - prev.y1;
            let same = (nx.size - prev.size).abs() < 0.15 * prev.size && gap < 0.9 * prev.size && !is_list(&nx.text);
            if !same || boxes.iter().any(|b| nx.y0 > b[1] && prev.y1 < b[1]) {
                break;
            }
            let t = charset::normalize(nx.text.trim());
            if para.ends_with('-') {
                para.pop();
                para.push_str(&t);
            } else {
                para.push(' ');
                para.push_str(&t);
            }
            j += 1;
        }
        let blk = if let Some((marker, rest)) = list_marker(&para) {
            Block::Li { text: rest, marker }
        } else {
            Block::P { text: para }
        };
        items.push((y0, blk));
        i = j;
    }
    items.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    items.into_iter().map(|(_, b)| b).collect()
}

fn is_list(t: &str) -> bool {
    list_marker(t).is_some()
}

fn list_marker(t: &str) -> Option<(String, String)> {
    let t = t.trim_start();
    let first = t.chars().next()?;
    if "•-–*▪◦".contains(first) {
        return Some(("-".into(), t[first.len_utf8()..].trim().to_string()));
    }
    let digits: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
    if !digits.is_empty() && digits.len() <= 3 {
        let rest = &t[digits.len()..];
        if rest.starts_with(". ") || rest.starts_with(") ") {
            return Some((format!("{digits}."), rest[2..].trim().to_string()));
        }
    }
    None
}

struct Line {
    text: String,
    y0: f32,
    y1: f32,
    size: f32,
}

/// Group characters into reading-order lines, marking raised/lowered small characters.
fn lines(mut chars: Vec<Ch>) -> Vec<Line> {
    chars.sort_by(|a, b| (a.y1).partial_cmp(&b.y1).unwrap());
    let mut rows: Vec<Vec<Ch>> = vec![];
    for c in chars {
        let tol = 0.5 * c.size.max(4.0);
        if let Some(r) = rows.iter_mut().rev().take(3).find(|r| {
            let base = crate::signal::median(&r.iter().map(|x| x.y1).collect::<Vec<_>>());
            (c.y1 - base).abs() < tol || ((c.y0 + c.y1) / 2.0 > r[0].y0 && (c.y0 + c.y1) / 2.0 < r[0].y1)
        }) {
            r.push(c);
        } else {
            rows.push(vec![c]);
        }
    }
    let mut out = vec![];
    for mut r in rows {
        r.sort_by(|a, b| a.x0.partial_cmp(&b.x0).unwrap());
        let size = crate::signal::median(&r.iter().map(|c| c.size).collect::<Vec<_>>());
        let base = crate::signal::median(&r.iter().filter(|c| c.size >= 0.85 * size).map(|c| c.y1).collect::<Vec<_>>());
        let mut s = String::new();
        let mut mode = 0; // 0 normal, 1 sup, 2 sub
        let mut prev_x1: Option<f32> = None;
        for c in &r {
            // spaces carry no position of their own: never let them open a super/subscript
            if c.c.is_whitespace() {
                if !s.ends_with(' ') {
                    s.push(' ');
                }
                prev_x1 = Some(c.x1);
                continue;
            }
            let m = if c.size < 0.8 * size && c.y1 < base - 0.2 * size {
                1
            } else if c.size < 0.8 * size && c.y1 > base + 0.1 * size {
                2
            } else {
                0
            };
            if let Some(px) = prev_x1 {
                if c.x0 - px > 0.25 * size && !s.ends_with(' ') && c.c != ' ' {
                    s.push(' ');
                }
            }
            if m != mode {
                if mode != 0 {
                    s.push(END);
                }
                if m == 1 {
                    s.push(SUP);
                } else if m == 2 {
                    s.push(SUB);
                }
                mode = m;
            }
            s.push(c.c);
            prev_x1 = Some(c.x1);
        }
        if mode != 0 {
            s.push(END);
        }
        let y0 = r.iter().map(|c| c.y0).fold(f32::MAX, f32::min);
        let y1 = r.iter().map(|c| c.y1).fold(f32::MIN, f32::max);
        out.push(Line { text: s, y0, y1, size });
    }
    out
}
