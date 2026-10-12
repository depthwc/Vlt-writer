//! Import handwriting photos into a Hand: filled sample-sheet pages, or any page + transcript.

use std::collections::HashMap;


use serde::Serialize;

use crate::geom;
use crate::hand::{Ctx, Hand, Src};
use crate::img::{self, Rgb};
use crate::matte::{self, Mode};
use crate::segment::{self, LineResult};
use crate::sheet::{self, Kind};
use crate::signal;

pub const SHEET_PPM: f64 = 15.0;

#[derive(Debug, Serialize, Default)]
pub struct Report {
    pub ok: bool,
    pub error: Option<String>,
    pub page: Option<usize>,
    pub boxes: usize,
    pub lines: usize,
    pub glyphs: usize,
    pub warnings: Vec<String>,
    /// id of this import (high bits of its words' ids)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub import: Option<u64>,
    /// how finely the photo shows the page, px per mm (the smallest of its photos): below
    /// about 8 the letters come out blurry
    #[serde(default)]
    pub px_mm: f32,
}

/// Below this many px per mm a photo gives blurry letters (a copy sent through a messenger).
pub const SHARP_PX_MM: f32 = 8.0;

#[derive(Default)]
struct Acc {
    gaps: Vec<f32>,
    words: Vec<f32>,
    xh: Vec<f32>,
    pairs: HashMap<String, [u32; 2]>,
}

/// `page`: a number for this import, so each written word gets an id of its own.
fn add_line(hand: &mut Hand, res: LineResult, ppm: f32, acc: &mut Acc, page: u64, line: usize) -> anyhow::Result<usize> {
    let n = res.glyphs.len();
    for c in res.glyphs {
        let id = hand.add_glyph(&c.text, &c.alpha, c.bl, ppm, Src::Text, false)?;
        if let Some(g) = hand.get_mut(&id) {
            // cut from joined writing: it joins again exactly where it was cut, rising to the right
            g.stubs = [c.stubs[0].map(|p| [p[0], p[1], -0.85, 0.5]), c.stubs[1].map(|p| [p[0], p[1], 0.85, -0.5])];
            g.ctx = Some(Ctx {
                word: page << 32 | (line as u64) << 16 | c.word as u64,
                pos: c.pos as u16,
                prev: c.prev,
                next: c.next,
                x: c.x,
                y: c.y,
            });
        }
    }
    acc.gaps.extend(res.gaps.iter().map(|g| g / ppm));
    acc.words.extend(res.words.iter().map(|g| g / ppm));
    acc.xh.extend(res.xh.iter().map(|g| g / ppm));
    for (p, [j, n]) in res.pairs {
        let e = acc.pairs.entry(p).or_insert([0, 0]);
        e[0] += j;
        e[1] += n;
    }
    Ok(n)
}

/// A random number for one imported page: the high bits of its words' ids.
fn import_id() -> u64 {
    uuid::Uuid::new_v4().as_u64_pair().0 & 0xFFFF_FFFF
}

fn merge_stats(hand: &mut Hand, acc: Acc) {
    let xh = if acc.xh.len() >= 5 { signal::median(&acc.xh) } else { hand.meta.xh_mm };
    let st = &mut hand.meta.stats;
    if acc.gaps.len() >= 8 {
        let g: Vec<f32> = acc.gaps.iter().map(|v| v / xh).collect();
        // robust spread (stray marks and odd cuts would inflate a plain standard deviation)
        st.gap = [signal::median(&g), signal::mad_sigma(&g).clamp(0.03, 0.2)];
    }
    if acc.words.len() >= 4 {
        let g: Vec<f32> = acc.words.iter().map(|v| v / xh).collect();
        st.word = [signal::median(&g), signal::mad_sigma(&g).clamp(0.1, 0.4)];
    }
    for (p, [j, n]) in acc.pairs {
        let e = st.pairs.entry(p).or_insert([0, 0]);
        e[0] += j;
        e[1] += n;
    }
    let tot: u32 = st.pairs.values().map(|v| v[1]).sum();
    if tot >= 10 {
        st.join = st.pairs.values().map(|v| v[0]).sum::<u32>() as f32 / tot as f32;
    }
}

fn update_ink(hand: &mut Hand, ink: [f32; 3], weight: usize) {
    if weight == 0 {
        return;
    }
    let n = hand.meta.ink_n;
    let w = weight as f32;
    hand.meta.ink = if n > 0.0 { [0, 1, 2].map(|c| (hand.meta.ink[c] * n + ink[c] * w) / (n + w)) } else { ink };
    hand.meta.ink_n = n + w;
}

/// One photo of a filled sheet page. `stored` finds pages kept in the library (sheets of
/// missing letters); every other page is rebuilt from its code.
pub fn import_sheet(hand: &mut Hand, photo: &Rgb, stored: &dyn Fn(sheet::Code) -> Option<sheet::Page>) -> anyhow::Result<Report> {
    let gray = photo.mean_channel();
    let Some((code, h)) = sheet::detect_page(&gray) else {
        return Ok(Report { error: Some("markers not found".into()), ..Default::default() });
    };
    let Some(p) = sheet::resolve(code).or_else(|| stored(code)) else {
        return Ok(Report { error: Some(format!("unknown page {code:?}")), ..Default::default() });
    };
    let page = p.index;
    let ppm = SHEET_PPM;
    let flat = geom::rectify(photo, &h, (sheet::PAGE_W, sheet::PAGE_H), ppm);
    let px_r = |x0: f64, y0: f64, x1: f64, y1: f64| [(x0 * ppm) as usize, (y0 * ppm) as usize, (x1 * ppm) as usize, (y1 * ppm) as usize];
    let mut printed: Vec<[usize; 4]> = sheet::MARKER_POS.iter().map(|m| px_r(m[0] - 1.0, m[1] - 1.0, m[0] + sheet::MARKER + 1.0, m[1] + sheet::MARKER + 1.0)).collect();
    let [hx0, hy0, hx1, hy1] = sheet::HEADER;
    printed.push(px_r(hx0, hy0, hx1, hy1)); // title and hints
    let (alpha, ink) = matte::ink_matte(&flat, ppm as f32, Mode::Sheet, &printed);
    let mut rep = Report { ok: true, page: Some(page + 1), ..Default::default() };
    let mut acc = Acc::default();
    let inset = 0.7;
    let px = |v: f64| (v * ppm) as usize;
    let import = import_id();
    for (ii, it) in p.items.iter().enumerate() {
        let [x, y, w, hh] = it.r;
        match it.kind {
            Kind::Glyph => {
                let (x0, y0, x1, y1) = (px(x + inset), px(y + inset), px(x + w - inset), px(y + hh - inset));
                let a = alpha.crop(x0, y0, x1, y1);
                let small = ".,'ʻ:;·-°".contains(it.text.as_str());
                let min_ink = if small { 0.06 } else { 0.6 } * ppm * ppm;
                if (a.d.iter().filter(|&&v| v > 0.5).count() as f64) < min_ink {
                    continue;
                }
                // ink touching the box edge suggests the letter was cut off
                let edge = (0..a.w).any(|xx| a.at(xx, 0) > 0.4 || a.at(xx, a.h - 1) > 0.4)
                    || (0..a.h).any(|yy| a.at(0, yy) > 0.4 || a.at(a.w - 1, yy) > 0.4);
                let Some((crop, _cx, cy)) = matte::tight_crop(&a, (0.6 * ppm) as usize) else { continue };
                let bl = (it.bl * ppm) as f32 - y0 as f32 - cy as f32;
                hand.add_glyph(&it.text, &crop, bl, ppm as f32, Src::Box, edge)?;
                rep.boxes += 1;
            }
            Kind::Line => {
                let a = alpha.crop(px(x), px(y), px(x + w), px(y + hh));
                if (a.d.iter().filter(|&&v| v > 0.5).count() as f64) < 2.0 * ppm * ppm {
                    continue;
                }
                let res = segment::harvest_line(&a, &it.text, ppm as f32, 0.2);
                rep.warnings.extend(res.warnings.clone());
                if add_line(hand, res, ppm as f32, &mut acc, import, ii)? > 0 {
                    rep.lines += 1;
                }
            }
        }
    }
    merge_stats(hand, acc);
    update_ink(hand, ink, rep.boxes + rep.lines);
    hand.meta.imports.push(serde_json::json!({"kind": "sheet", "code": code, "page": page, "boxes": rep.boxes, "lines": rep.lines}));
    hand.refresh();
    rep.glyphs = rep.boxes;
    Ok(rep)
}

/// Leaves out letters that cannot be used (`hand::unusable`), judged at the photo's x-height.
fn drop_unusable(results: &mut [LineResult]) {
    let v: Vec<f32> = results.iter().flat_map(|r| r.xh.iter().copied()).collect();
    if v.len() < 3 {
        return;
    }
    let xh = signal::median(&v);
    for r in results.iter_mut() {
        r.glyphs.retain(|c| !crate::hand::unusable(crate::hand::features(&c.alpha).sw, xh));
    }
    // far more ink than the photo's other versions of the same character
    let mut ink: HashMap<&str, Vec<f32>> = HashMap::new();
    for c in results.iter().flat_map(|r| r.glyphs.iter()) {
        ink.entry(c.text.as_str()).or_default().push(crate::hand::ink_area(&c.alpha, xh));
    }
    let usual: HashMap<String, f32> = ink.into_iter().filter(|(_, v)| v.len() >= 8).map(|(k, v)| (k.to_string(), signal::median(&v))).collect();
    for r in results.iter_mut() {
        r.glyphs.retain(|c| usual.get(&c.text).is_none_or(|m| crate::hand::ink_area(&c.alpha, xh) <= crate::hand::HEAVY * m));
    }
}

/// Versions that look unlike the other versions of the same character (a wrong cut, most
/// likely): (line, glyph) indices. Compared on a small picture with the baseline aligned.
fn odd_ones(results: &[LineResult], ppm: f32) -> std::collections::HashSet<(usize, usize)> {
    let xh = {
        let v: Vec<f32> = results.iter().flat_map(|r| r.xh.iter().copied()).collect();
        if v.is_empty() { 2.5 * ppm } else { signal::median(&v) }
    };
    let (w, h, base) = (20usize, 28usize, 18.0f32);
    let k = 8.0 / xh.max(1.0);
    let thumb = |a: &crate::img::Gray, bl: f32| -> Vec<f32> {
        let mut t = vec![0f32; w * h];
        let cols: Vec<usize> = (0..a.w).filter(|&x| (0..a.h).any(|y| a.at(x, y) > 0.3)).collect();
        let x0 = cols.first().copied().unwrap_or(0) as f32;
        for (i, v) in t.iter_mut().enumerate() {
            let (tx, ty) = ((i % w) as f32, (i / w) as f32);
            *v = a.sample(x0 + (tx - 1.0) / k, bl + (ty - base) / k);
        }
        let blurred = crate::img::blur(&crate::img::Gray { w, h, d: t }, 1.0);
        let m = signal::mean(&blurred.d);
        blurred.d.iter().map(|v| v - m).collect()
    };
    let corr = |a: &[f32], b: &[f32]| -> f32 {
        let ab: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
        let aa: f32 = a.iter().map(|x| x * x).sum();
        let bb: f32 = b.iter().map(|x| x * x).sum();
        ab / (aa * bb).sqrt().max(1e-9)
    };
    let mut by: HashMap<&str, Vec<((usize, usize), Vec<f32>)>> = HashMap::new();
    for (li, r) in results.iter().enumerate() {
        for (gi, c) in r.glyphs.iter().enumerate() {
            by.entry(c.text.as_str()).or_default().push(((li, gi), thumb(&c.alpha, c.bl)));
        }
    }
    let mut odd = std::collections::HashSet::new();
    // blobs and scribbles: strokes far thicker than the pen
    let sws: Vec<((usize, usize), f32)> = results
        .iter()
        .enumerate()
        .flat_map(|(li, r)| r.glyphs.iter().enumerate().map(move |(gi, c)| ((li, gi), crate::hand::features(&c.alpha).sw)))
        .collect();
    if sws.len() >= 10 {
        let med = signal::median(&sws.iter().map(|x| x.1).collect::<Vec<_>>());
        odd.extend(sws.iter().filter(|x| x.1 > 1.9 * med).map(|x| x.0));
    }
    for list in by.values().filter(|l| l.len() >= 4) {
        // typical version: the one most like all others
        let score: Vec<f32> = list.iter().map(|(_, a)| signal::mean(&list.iter().map(|(_, b)| corr(a, b)).collect::<Vec<_>>())).collect();
        let best = (0..list.len()).max_by(|&i, &j| score[i].partial_cmp(&score[j]).unwrap()).unwrap();
        let sims: Vec<f32> = list.iter().map(|(_, a)| corr(a, &list[best].1)).collect();
        let med = signal::median(&sims);
        let mad = signal::median(&sims.iter().map(|s| (s - med).abs()).collect::<Vec<_>>()).max(0.03);
        for (i, (id, _)) in list.iter().enumerate() {
            if sims[i] < (med - 3.0 * mad).min(0.55) {
                odd.insert(*id);
            }
        }
    }
    odd
}

/// The person's typical letters of one style (joined or print): per character, the versions
/// most like the others, from the hand's written letters and a first cut of the new page.
fn typical_shapes(hand: &Hand, first: &[segment::LineResult], print: bool) -> HashMap<char, Vec<Vec<f32>>> {
    let mut by: HashMap<char, Vec<Vec<f32>>> = HashMap::new();
    let xh_mm = hand.meta.xh_mm;
    for g in hand.glyphs.iter().filter(|g| g.on && g.src == crate::hand::Src::Text && (g.print == print || !crate::hand::has_letter(&g.text))) {
        let mut cs = g.text.chars();
        if let (Some(c), None) = (cs.next(), cs.next()) {
            let list = by.entry(c).or_default();
            if list.len() < 40 {
                list.push(crate::hand::shape(&hand.alpha(&g.id), g.bl, xh_mm * g.ppm * hand.page_scale(g)));
            }
        }
    }
    for r in first {
        let xh = if r.xh.is_empty() { None } else { Some(signal::median(&r.xh)) };
        for c in &r.glyphs {
            let mut cs = c.text.chars();
            if let (Some(ch), None, Some(xh)) = (cs.next(), cs.next(), xh) {
                by.entry(ch).or_default().push(crate::hand::shape(&c.alpha, c.bl, xh));
            }
        }
    }
    by.into_iter()
        .filter(|(_, l)| l.len() >= 4)
        .map(|(c, l)| {
            let t = crate::hand::typicality(&l);
            let mut v: Vec<(f32, Vec<f32>)> = t.into_iter().zip(l).filter(|(s, _)| *s >= 0.7).collect();
            v.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
            (c, v.into_iter().take(10).map(|x| x.1).collect())
        })
        .collect()
}

/// A photo made ready for cutting: the ink, its scale and the rows of writing on it.
struct Prepared {
    alpha: crate::img::Gray,
    ink: [f32; 3],
    ppm: f64,
    /// px per mm of the photo itself (before it was looked at smaller)
    photo_ppm: f64,
    found: bool,
    grid: bool,
}

/// The cutter works at about this many px per mm (the scale it was tuned on): a big phone
/// photo is looked at smaller, which is also much faster.
const WORK_PPM: f64 = 10.0;

fn prepare(photo: &Rgb, size: &str) -> Prepared {
    let (corners, found) = geom::detect_sheet(photo);
    let mut ppm = 12.0f64;
    let mut flat = if found {
        let sz = geom::paper_size(size);
        geom::rectify(photo, &geom::mm_to_px(&corners, sz), sz, ppm)
    } else {
        photo.clone()
    };
    // squared paper: its 5 mm squares give the scale (also when the guessed sheet size is off)
    let grid = geom::grid_pitch(&flat);
    if let Some(p) = grid {
        ppm = p / 5.0;
    }
    if !found && grid.is_none() {
        // nothing to measure against: assume ~3 mm letters
        let (alpha, _) = matte::ink_matte(&flat, ppm as f32, Mode::Generic, &[]);
        let m: Vec<bool> = alpha.d.iter().map(|&v| v > 0.35).collect();
        let (_, st) = img::components(&m, alpha.w, alpha.h);
        let hs: Vec<f32> = st.iter().skip(1).map(|c| c.h() as f32).collect();
        if !hs.is_empty() {
            let p40 = signal::percentile(&hs, 40.0);
            ppm = (signal::median(&hs.iter().copied().filter(|&v| v > p40).collect::<Vec<_>>()) / 3.0).max(3.0) as f64;
        }
    }
    let photo_ppm = ppm;
    if ppm > WORK_PPM * 1.15 {
        let (w, h) = ((flat.w as f64 * WORK_PPM / ppm).round() as usize, (flat.h as f64 * WORK_PPM / ppm).round() as usize);
        ppm *= w as f64 / flat.w as f64;
        flat = flat.resize(w, h);
    }
    let (mut alpha, ink) = matte::ink_matte(&flat, ppm as f32, Mode::Generic, &[]);
    // what is not paper (the desk around the page, a dark shadow): no ink there
    let bg = matte::background(&flat.min_channel(), ppm as f32);
    let step = (bg.d.len() / 50_000).max(1);
    let usual = signal::median(&bg.d.iter().step_by(step).copied().collect::<Vec<_>>());
    for (a, b) in alpha.d.iter_mut().zip(&bg.d) {
        if *b < 0.55 * usual {
            *a = 0.0;
        }
    }
    Prepared { alpha: segment::deskew(&alpha), ink, ppm, photo_ppm, found, grid: grid.is_some() }
}

/// Any photo of handwriting plus the exact text on it, one line per line.
pub fn import_photo(hand: &mut Hand, photo: &Rgb, transcript: &str, size: &str) -> anyhow::Result<Report> {
    import_photos(hand, &[photo], transcript, size)
}

/// One page of writing over several photos (it did not fit on one sheet, or was photographed
/// in parts), in order, plus its text: the lines go on from one photo to the next, and a line
/// may break where a photo ends.
pub fn import_photos(hand: &mut Hand, photos: &[&Rgb], transcript: &str, size: &str) -> anyhow::Result<Report> {
    import_styled(hand, photos, transcript, size, &[])
}

/// `import_photos` where some lines are written in print letters: `print` says which, per
/// line of the transcript that holds text (a line not given is joined writing). Each style is
/// cut and judged against the person's letters of that style.
pub fn import_styled(hand: &mut Hand, photos: &[&Rgb], transcript: &str, size: &str, print: &[bool]) -> anyhow::Result<Report> {
    if photos.is_empty() {
        return Ok(Report { error: Some("no photo".into()), ..Default::default() });
    }
    let preps: Vec<Prepared> = photos.iter().map(|ph| prepare(ph, size)).collect();
    import_prepared(hand, preps, transcript, print)
}

/// A page whose ink was found some other way (a clean matte, `ppm` px per mm): cut and added as
/// `import_photos` does.
pub fn import_alpha(hand: &mut Hand, alpha: crate::img::Gray, ppm: f64, transcript: &str, print: &[bool]) -> anyhow::Result<Report> {
    let prep = Prepared { alpha: segment::deskew(&alpha), ink: hand.meta.ink, ppm, photo_ppm: ppm, found: false, grid: true };
    import_prepared(hand, vec![prep], transcript, print)
}

fn import_prepared(hand: &mut Hand, preps: Vec<Prepared>, transcript: &str, print: &[bool]) -> anyhow::Result<Report> {
    let lines: Vec<&str> = transcript.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() {
        return Ok(Report { error: Some("empty transcript".into()), ..Default::default() });
    }
    let photos = &preps;
    let _ = (preps[0].found, preps[0].grid);
    // the rows of every photo, in order; (photo, row image)
    let rows: Vec<(usize, crate::img::Gray)> = if preps.len() == 1 {
        // two ways to find the lines: every row of ink, or as many rows as there are lines of
        // text; keep the one whose widths fit the text better
        let pr = &preps[0];
        let all = segment::find_lines(&pr.alpha, pr.ppm as f32);
        let (_, ca) = segment::match_lines(&all, &lines);
        let fixed = segment::split_lines(&pr.alpha, lines.len(), pr.ppm as f32);
        let (_, cb) = segment::match_lines(&fixed, &lines);
        // more rows of writing than lines of text (lines that went on to the next row): as
        // many bands as lines would put two rows in one
        let ws: Vec<f64> = all.iter().map(segment::row_width).collect();
        let med = signal::median(&ws.iter().map(|&w| w as f32).collect::<Vec<_>>()) as f64;
        // a row with a word on it counts (a line's last word on the next row), not a speck
        let rows = ws.iter().filter(|&&w| w >= 0.05 * med).count();
        if std::env::var("HW_DEBUG_CUTS").is_ok() {
            eprintln!("{} / {} lines found ({rows} with writing), {} typed; costs {ca:.2} / {cb:.2}", all.len(), fixed.len(), lines.len());
        }
        (if cb < ca && rows <= lines.len() { fixed } else { all }).into_iter().map(|g| (0, g)).collect()
    } else {
        preps.iter().enumerate().flat_map(|(i, pr)| segment::find_lines(&pr.alpha, pr.ppm as f32).into_iter().map(move |g| (i, g))).collect()
    };
    // widths in mm, so photos taken nearer or farther compare
    let spans: Vec<(f64, f64)> = rows
        .iter()
        .map(|(i, g)| {
            let (a, b) = segment::row_span(g);
            (a / preps[*i].ppm, b / preps[*i].ppm)
        })
        .collect();
    let starts: Vec<usize> = (1..rows.len()).filter(|&k| rows[k].0 != rows[k - 1].0).collect();
    let (pairs, _) = segment::match_widths(&spans, &starts, &lines);
    let px_mm = preps.iter().map(|p| p.photo_ppm as f32).fold(f32::MAX, f32::min);
    let mut rep = Report { ok: true, px_mm, ..Default::default() };
    let missed: Vec<usize> = (0..lines.len()).filter(|i| !pairs.iter().any(|p| p.1 == *i)).collect();
    for i in &missed {
        rep.warnings.push(format!("line not found: {}", lines[*i]));
    }
    if std::env::var("HW_DEBUG_CUTS").is_ok() {
        eprintln!("{} rows over {} photos; pairs {:?}", rows.len(), photos.len(), pairs);
    }
    // a line written over two rows is two pieces of text; (photo, row, text, in print)
    let pieces: Vec<(usize, &crate::img::Gray, &str, bool)> = pairs.iter().map(|p| (rows[p.0].0, &rows[p.0].1, p.2.as_str(), print.get(p.1).copied().unwrap_or(false))).collect();
    let mut acc = Acc::default();
    // first pass: this person's letter widths, joined and print apart (joined writing makes
    // narrow letters wider)
    let mut widths = [segment::Widths::default(), segment::Widths::default()];
    for (i, a, text, p) in &pieces {
        segment::learn_widths(a, text, preps[*i].ppm as f32, &mut widths[*p as usize]);
    }
    widths.iter_mut().for_each(|w| w.finish());
    // second pass: the person's typical letters of the style (from this page and the hand)
    // guide the cuts
    let mut first = [vec![], vec![]];
    for (i, a, text, p) in &pieces {
        first[*p as usize].push(segment::harvest(a, text, preps[*i].ppm as f32, 0.2, &widths[*p as usize], None));
    }
    for (s, w) in widths.iter_mut().enumerate() {
        w.shapes = typical_shapes(hand, &first[s], s == 1);
    }
    let mut results = vec![];
    for (i, a, text, p) in &pieces {
        let res = segment::harvest(a, text, preps[*i].ppm as f32, 0.2, &widths[*p as usize], None);
        rep.warnings.extend(res.warnings.clone());
        results.push((*i, *p, res));
    }
    let import = import_id();
    // the lines of each photo and style: letters are judged among those of the same photo
    // written the same way
    let mut groups: Vec<(usize, bool, Vec<segment::LineResult>)> = vec![];
    for (i, p, r) in results {
        match groups.iter_mut().find(|g| g.0 == i && g.1 == p) {
            Some(g) => g.2.push(r),
            None => groups.push((i, p, vec![r])),
        }
    }
    let mut li = 0;
    for (i, p, mut mine) in groups {
        let pr = &preps[i];
        drop_unusable(&mut mine);
        let odd = odd_ones(&mine, pr.ppm as f32);
        for (k, res) in mine.into_iter().enumerate() {
            let before = hand.glyphs.len();
            rep.glyphs += add_line(hand, res, pr.ppm as f32, &mut acc, import, li)?;
            for (gi, g) in hand.glyphs[before..].iter_mut().enumerate() {
                if odd.contains(&(k, gi)) {
                    g.odd = true;
                    g.on = false;
                }
                g.print = p && crate::hand::has_letter(&g.text);
            }
            li += 1;
        }
    }
    merge_stats(hand, acc);
    update_ink(hand, preps[0].ink, rep.glyphs);
    // what the person writes in: add the languages of this page
    for l in crate::charset::guess_langs(transcript) {
        if !hand.meta.langs.contains(&l) {
            hand.meta.langs.push(l);
        }
    }
    hand.meta.imports.push(serde_json::json!({"kind": "photo", "glyphs": rep.glyphs, "import": import, "px_mm": rep.px_mm}));
    rep.import = Some(import);
    hand.refresh();
    rep.warnings.truncate(20);
    Ok(rep)
}
