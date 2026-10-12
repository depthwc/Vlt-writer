//! Printable sample sheet: AprilTag-marked A4 pages with boxes for letters, pairs and text lines.
//!
//! What a sheet asks for follows the person's languages and how thorough they want to be.
//! Geometry is in millimetres and fully deterministic. The four corner markers carry a page
//! code [a, b] (top-left and bottom-right markers carry a, the other two b, so any three
//! markers identify the page):
//! - a = b < 8: page a of the first fixed sheet (v0.2)
//! - 8 <= a < 136: page b of the sheet made from recipe a - 8 (languages + level)
//! - 136 <= a < 146: a stored page (sheets of missing letters), looked up in the library

use std::collections::HashMap;
use std::sync::Arc;

use krilla::color::rgb;
use krilla::geom::{PathBuilder, Point, Rect};
use krilla::num::NormalizedF32;
use krilla::page::PageSettings;
use krilla::paint::{Fill, Stroke};
use krilla::text::{Font, TextDirection};
use krilla::Document;
use serde::{Deserialize, Serialize};

use crate::charset::{self, Lang, Level};
use crate::geom::P2;
use crate::img::Gray;

pub const PAGE_W: f64 = 210.0;
pub const PAGE_H: f64 = 297.0;
pub const MARKER: f64 = 14.0;
/// Top-left corner of each marker: TL, TR, BR, BL.
pub const MARKER_POS: [P2; 4] = [[8.0, 8.0], [188.0, 8.0], [188.0, 275.0], [8.0, 275.0]];
/// Printed title and hints between the top markers (never read as handwriting).
pub const HEADER: [f64; 4] = [23.0, 9.0, 187.0, 25.5];
const CONTENT: [f64; 4] = [10.0, 27.0, 200.0, 272.0];
const BOX_W: f64 = 13.0;
const BOX_H: f64 = 15.0;
const LABEL_H: f64 = 3.5;
const CELL_W: f64 = 14.5;
const CELL_H: f64 = BOX_H + LABEL_H + 1.0;
const BASELINE_FROM_TOP: f64 = 10.0;
const PAIR_W: f64 = 20.0;
const PAIR_CELL_W: f64 = 21.5;
const LINE_PITCH: f64 = 16.0;
/// Pastel dropout colour: bright in the red channel, so it vanishes from the matte.
const GUIDE: (u8, u8, u8) = (255, 219, 204);
const LABEL: (u8, u8, u8) = (219, 128, 112);
const MM: f32 = 72.0 / 25.4;

/// Page code carried by the markers.
pub type Code = [u16; 2];
const FIRST_SHEET: u16 = 8;
const RECIPES: u16 = 8;
const STORED: u16 = 136;
const ROW: u16 = 146;
pub const STORED_MAX: u32 = (ROW - STORED) as u32 * ROW as u32;

pub fn stored_code(n: u32) -> Code {
    let n = n % STORED_MAX;
    [STORED + (n / ROW as u32) as u16, (n % ROW as u32) as u16]
}

pub fn is_stored(c: Code) -> bool {
    (STORED..ROW).contains(&c[0]) && c[1] < ROW
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Glyph,
    Line,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Item {
    pub kind: Kind,
    pub text: String,
    /// x, y, w, h in mm
    pub r: [f64; 4],
    /// baseline y in mm
    pub bl: f64,
}

/// What a page asks for (also picks the printed hint).
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PageKind {
    Latin,
    Cyrillic,
    Digits,
    Math,
    /// joined pairs and lines to copy in one language
    Words,
    /// letters that were missing or need redoing
    Refill,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Page {
    /// position in its sheet
    pub index: usize,
    pub code: Code,
    pub kind: PageKind,
    #[serde(default)]
    pub lang: Option<Lang>,
    pub items: Vec<Item>,
}

impl Page {
    /// Key for the page title in the UI texts: "latin", "words.uz_latn", ...
    pub fn key(&self) -> String {
        let k = serde_json::to_value(self.kind).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
        match self.lang {
            Some(l) => format!("{k}.{}", l.key()),
            None => k,
        }
    }
}

/// Boxes in rows, then lines to copy; a new page whenever one is full.
fn flow(boxes: &[(String, usize)], cell_w: f64, box_w: f64, lines: &[&str]) -> Vec<Vec<Item>> {
    let [x0, y0, x1, y1] = CONTENT;
    let cols = ((x1 - x0) / cell_w).floor() as usize;
    let mut pages = vec![];
    let mut cur: Vec<Item> = vec![];
    let (mut y, mut col) = (y0, 0usize);
    for (text, n) in boxes {
        for _ in 0..*n {
            if col == cols {
                col = 0;
                y += CELL_H;
            }
            if y + CELL_H > y1 + 1e-6 {
                pages.push(std::mem::take(&mut cur));
                y = y0;
            }
            let (cx, cy) = (x0 + col as f64 * cell_w, y + LABEL_H);
            cur.push(Item { kind: Kind::Glyph, text: text.clone(), r: [cx, cy, box_w, BOX_H], bl: cy + BASELINE_FROM_TOP });
            col += 1;
        }
    }
    if col > 0 {
        y += CELL_H;
    }
    for t in lines {
        if y + 11.0 + 4.5 > y1 {
            pages.push(std::mem::take(&mut cur));
            y = y0;
        }
        let bl = y + 11.0;
        cur.push(Item { kind: Kind::Line, text: t.to_string(), r: [x0, bl - 9.0, x1 - x0, 13.5], bl });
        y += LINE_PITCH;
    }
    if !cur.is_empty() {
        pages.push(cur);
    }
    pages
}

/// The first fixed sheet (v0.2): eight pages, Latin, Cyrillic, digits, maths, pairs, text.
pub fn first_sheet() -> Vec<Page> {
    let single = |s: &str| -> Vec<(String, usize)> {
        s.chars().filter(|c| charset::alias_of(*c).is_none()).map(|c| (c.to_string(), charset::variants_for(c))).collect()
    };
    let parts: Vec<(PageKind, Vec<Vec<Item>>)> = vec![
        (PageKind::Latin, flow(&single(&[charset::LATIN_LOWER, charset::LATIN_UPPER].concat()), CELL_W, BOX_W, &[])),
        (PageKind::Cyrillic, flow(&single(&[charset::CYR_LOWER, charset::CYR_UPPER].concat()), CELL_W, BOX_W, &[])),
        (PageKind::Digits, flow(&single(&[charset::DIGITS, charset::PUNCT].concat()), CELL_W, BOX_W, &[])),
        (PageKind::Math, flow(&single(charset::MATH), CELL_W, BOX_W, &[])),
        (PageKind::Words, flow(&charset::UZ_PAIRS.iter().chain(charset::RU_PAIRS).map(|p| (p.to_string(), 2)).collect::<Vec<_>>(), PAIR_CELL_W, PAIR_W, &[])),
        (PageKind::Words, flow(&[], CELL_W, BOX_W, charset::TEXT_LINES)),
    ];
    let mut pages = vec![];
    for (kind, ps) in parts {
        for items in ps {
            let i = pages.len() as u16;
            pages.push(Page { index: pages.len(), code: [i, i], kind, lang: None, items });
        }
    }
    pages
}

/// Which languages and how thorough: everything a sheet needs, small enough for the markers.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Recipe {
    pub langs: Vec<Lang>,
    #[serde(default)]
    pub level: Level,
}

impl Recipe {
    pub fn id(&self) -> u16 {
        let mask: u16 = self.langs.iter().map(|l| l.bit()).fold(0, |a, b| a | b);
        mask | (Level::ALL.iter().position(|l| *l == self.level).unwrap() as u16) << 5
    }

    pub fn from_id(id: u16) -> Option<Recipe> {
        let level = *Level::ALL.get((id >> 5) as usize)?;
        Some(Recipe { langs: Lang::ALL.into_iter().filter(|l| id & l.bit() != 0).collect(), level })
    }
}

/// Pages for a recipe: letters per script, pairs and lines per language, digits, maths.
pub fn recipe_pages(r: &Recipe) -> Vec<Page> {
    let mut langs = r.langs.clone();
    langs.sort();
    langs.dedup();
    let lv = r.level;
    let mut parts: Vec<(PageKind, Option<Lang>, Vec<Vec<Item>>)> = vec![];
    for (group, letters) in charset::letters_for(&langs) {
        let boxes: Vec<(String, usize)> = letters.iter().map(|&(c, rank)| (c.to_string(), lv.letter_versions(rank, c.is_uppercase()))).collect();
        let kind = if group == "latin" { PageKind::Latin } else { PageKind::Cyrillic };
        parts.push((kind, None, flow(&boxes, CELL_W, BOX_W, &[])));
    }
    let (np, nv, nl) = lv.words();
    for &l in langs.iter().filter(|l| **l != Lang::Math) {
        let pairs: Vec<(String, usize)> = l.pairs().iter().take(np).map(|p| (p.to_string(), nv)).collect();
        let lines: Vec<&str> = l.lines().iter().take(nl).copied().collect();
        parts.push((PageKind::Words, Some(l), flow(&pairs, PAIR_CELL_W, PAIR_W, &lines)));
    }
    let syms: Vec<(String, usize)> = charset::DIGITS.chars().chain(charset::punct_for(&langs)).map(|c| (c.to_string(), lv.symbol_versions(c))).collect();
    parts.push((PageKind::Digits, None, flow(&syms, CELL_W, BOX_W, charset::DIGIT_LINES)));
    if langs.contains(&Lang::Math) {
        let m: Vec<(String, usize)> = charset::MATH.chars().map(|c| (c.to_string(), lv.symbol_versions(c))).collect();
        parts.push((PageKind::Math, None, flow(&m, CELL_W, BOX_W, &[])));
    }
    let a = RECIPES + Recipe { langs, level: lv }.id();
    let mut pages = vec![];
    for (kind, lang, ps) in parts {
        for items in ps {
            pages.push(Page { index: pages.len(), code: [a, pages.len() as u16], kind, lang, items });
        }
    }
    pages
}

/// Pages asking again for letters that are missing or need redoing: (character, boxes).
/// Codes are given when the pages are stored.
pub fn refill_pages(chars: &[(String, usize)]) -> Vec<Page> {
    flow(chars, CELL_W, BOX_W, &[])
        .into_iter()
        .enumerate()
        .map(|(i, items)| Page { index: i, code: [0, 0], kind: PageKind::Refill, lang: None, items })
        .collect()
}

/// The page a code stands for, unless it is a stored page (the library knows those).
pub fn resolve(code: Code) -> Option<Page> {
    let [a, b] = code;
    if a < FIRST_SHEET {
        return if a == b { first_sheet().into_iter().nth(a as usize) } else { None };
    }
    if a < STORED {
        return recipe_pages(&Recipe::from_id(a - RECIPES)?).into_iter().nth(b as usize);
    }
    None
}

pub fn tag_id(code: Code, slot: usize) -> usize {
    4 * code[slot % 2] as usize + slot
}

pub fn marker_corners_mm(slot: usize) -> [P2; 4] {
    let [x, y] = MARKER_POS[slot];
    // corners of the black square (1 cell inside the white quiet zone), TL TR BR BL
    let c = MARKER / 10.0;
    [[x + c, y + c], [x + MARKER - c, y + c], [x + MARKER - c, y + MARKER - c], [x + c, y + MARKER - c]]
}

pub fn marker_center_mm(slot: usize) -> P2 {
    let [x, y] = MARKER_POS[slot];
    [x + MARKER / 2.0, y + MARKER / 2.0]
}

/// tag36h11 bit pattern as a 10x10 grid (true = black), C apriltag layout.
pub fn tag_cells(id: usize) -> [[bool; 10]; 10] {
    let fam = kornia_apriltag::family::TagFamily::tag36_h11();
    let code = fam.code_data[id] as u64;
    let mut g = [[true; 10]; 10];
    for i in 0..10 {
        g[0][i] = false;
        g[9][i] = false;
        g[i][0] = false;
        g[i][9] = false;
    }
    for i in 0..fam.nbits {
        if code & (1u64 << (fam.nbits - i - 1)) != 0 {
            let (x, y) = (fam.bit_x[i] as usize + 1, fam.bit_y[i] as usize + 1);
            g[y][x] = false;
        }
    }
    g
}

fn color(c: (u8, u8, u8)) -> krilla::paint::Paint {
    rgb::Color::new(c.0, c.1, c.2).into()
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> Option<krilla::geom::Path> {
    let mut pb = PathBuilder::new();
    pb.push_rect(Rect::from_xywh(x as f32 * MM, y as f32 * MM, w as f32 * MM, h as f32 * MM)?);
    pb.finish()
}

fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> Option<krilla::geom::Path> {
    let mut pb = PathBuilder::new();
    pb.move_to(x0 as f32 * MM, y0 as f32 * MM);
    pb.line_to(x1 as f32 * MM, y1 as f32 * MM);
    pb.finish()
}

/// Printed words, in the person's interface language.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct SheetText {
    /// on every page: pen, size, skipping
    pub head: String,
    /// pages of single boxes
    pub boxes: String,
    /// pages with pairs and lines
    pub words: String,
    pub refill: String,
    /// page titles by `Page::key()`
    pub titles: HashMap<String, String>,
}

impl Default for SheetText {
    fn default() -> Self {
        SheetText {
            head: "Your usual pen and size · not red, not pencil · empty box = skip".into(),
            boxes: "Write the character shown above each box, a little differently each time. Sit on the line, keep off the edges.".into(),
            words: "Pairs: write them joined, as in a word. Lines: copy each line onto the line below it, at your usual speed.".into(),
            refill: "Letters that were missing or need another go. Write each one as usual.".into(),
            titles: HashMap::new(),
        }
    }
}

/// Font size (pt) at which `text` fits `max_mm`, at most `size`.
fn fit(font: &ab_glyph::FontRef, text: &str, size: f32, max_mm: f32) -> f32 {
    use ab_glyph::Font as _;
    let upm = font.units_per_em().unwrap_or(1000.0);
    let w_pt: f32 = text.chars().map(|c| font.h_advance_unscaled(font.glyph_id(c)) / upm * size).sum();
    let w_mm = w_pt / MM;
    if w_mm > max_mm { size * max_mm / w_mm } else { size }
}

/// Render pages to PDF bytes. `font` is any TTF covering Latin + Cyrillic (e.g. Arial).
pub fn render_pdf(pages: &[Page], font_data: Vec<u8>, text: &SheetText) -> anyhow::Result<Vec<u8>> {
    let metrics = ab_glyph::FontRef::try_from_slice(&font_data).map_err(|_| anyhow::anyhow!("bad font"))?;
    let font = Font::new(Arc::new(font_data.clone()).into(), 0).ok_or_else(|| anyhow::anyhow!("bad font"))?;
    let mut doc = Document::new();
    let total = pages.len();
    let fill = |c| Some(Fill { paint: color(c), opacity: NormalizedF32::ONE, rule: Default::default() });
    for (pi, p) in pages.iter().enumerate() {
        let mut page = doc.start_page_with(PageSettings::from_wh(PAGE_W as f32 * MM, PAGE_H as f32 * MM).unwrap());
        let mut s = page.surface();
        for slot in 0..4 {
            let cells = tag_cells(tag_id(p.code, slot));
            let [mx, my] = MARKER_POS[slot];
            let c = MARKER / 10.0;
            s.set_fill(fill((0, 0, 0)));
            s.set_stroke(None);
            for (y, row) in cells.iter().enumerate() {
                for (x, &black) in row.iter().enumerate() {
                    if black {
                        if let Some(r) = rect(mx + x as f64 * c, my + y as f64 * c, c + 0.02, c + 0.02) {
                            s.draw_path(&r);
                        }
                    }
                }
            }
        }
        let title = text.titles.get(&p.key()).cloned().unwrap_or_else(|| p.key());
        let hint = match p.kind {
            PageKind::Words => &text.words,
            PageKind::Refill => &text.refill,
            _ => &text.boxes,
        };
        let max_w = (HEADER[2] - HEADER[0] - 4.0) as f32;
        let x = (HEADER[0] + 2.0) as f32 * MM;
        let rows = [(format!("Vlt-writer  ·  {}/{}  ·  {title}", pi + 1, total), 10.0, 13.8, (50, 50, 50)), (hint.clone(), 8.0, 18.6, (80, 80, 80)), (text.head.clone(), 8.0, 22.8, (80, 80, 80))];
        for (t, size, y, c) in rows {
            if t.is_empty() {
                continue;
            }
            s.set_fill(fill(c));
            s.draw_text(Point::from_xy(x, y * MM), font.clone(), fit(&metrics, &t, size, max_w), &t, false, TextDirection::Auto);
        }
        for it in &p.items {
            let [x, y, w, h] = it.r;
            s.set_fill(None);
            s.set_stroke(Some(Stroke { paint: color(GUIDE), width: 0.6, ..Default::default() }));
            if it.kind == Kind::Glyph {
                if let Some(r) = rect(x, y, w, h) {
                    s.draw_path(&r);
                }
            }
            if let Some(l) = line(x, it.bl, x + w, it.bl) {
                s.draw_path(&l);
            }
            s.set_stroke(None);
            s.set_fill(fill(LABEL));
            let (ty, size) = if it.kind == Kind::Glyph { (y - 0.8, 8.0) } else { (y - 0.6, 9.0) };
            s.draw_text(Point::from_xy(x as f32 * MM, ty as f32 * MM), font.clone(), size, &it.text, false, TextDirection::Auto);
        }
        s.finish();
        page.finish();
    }
    doc.finish().map_err(|e| anyhow::anyhow!("pdf: {e:?}"))
}

/// Detected tags as (id, black-square corners TL TR BR BL in px) at one scale.
fn detect_tags(gray: &Gray, max_side: usize) -> Vec<(usize, [P2; 4])> {
    use kornia_apriltag::family::TagFamilyKind;
    use kornia_apriltag::{AprilTagDecoder, DecodeTagsConfig};
    use kornia_image::{allocator::CpuAllocator, Image, ImageSize};

    let k = (max_side as f64 / gray.w.max(gray.h) as f64).min(1.0);
    let small;
    let g = if k < 1.0 {
        small = gray.resize((gray.w as f64 * k) as usize, (gray.h as f64 * k) as usize);
        &small
    } else {
        gray
    };
    let data: Vec<u8> = g.d.iter().map(|v| (v.clamp(0.0, 1.0) * 255.0) as u8).collect();
    let size = ImageSize { width: g.w, height: g.h };
    let Ok(img) = Image::<u8, 1, CpuAllocator>::new(size, data, CpuAllocator) else { return vec![] };
    let Ok(mut dec) = AprilTagDecoder::new(DecodeTagsConfig::new(vec![TagFamilyKind::Tag36H11]), size) else { return vec![] };
    let Ok(dets) = dec.decode(&img) else { return vec![] };
    dets.iter()
        .filter(|d| d.hamming <= 2)
        .map(|d| {
            // detector order for an upright tag is TR, BR, BL, TL
            let c = d.quad.corners.map(|p| [p.x as f64 / k, p.y as f64 / k]);
            (d.id as usize, [c[3], c[0], c[1], c[2]])
        })
        .collect()
}

/// Best page code among detected tags: (code, [(slot, corners)]).
fn match_code(tags: &[(usize, [P2; 4])]) -> Option<(Code, Vec<(usize, [P2; 4])>)> {
    let tags: Vec<(usize, u16, [P2; 4])> = tags.iter().filter(|t| t.0 < 4 * ROW as usize).map(|(id, c)| (id % 4, (id / 4) as u16, *c)).collect();
    let vals = |odd: usize| -> Vec<u16> {
        let mut v: Vec<u16> = tags.iter().filter(|t| t.0 % 2 == odd).map(|t| t.1).collect();
        v.sort();
        v.dedup();
        v
    };
    let mut best: Option<(Code, Vec<(usize, [P2; 4])>)> = None;
    for a in vals(0) {
        for b in vals(1) {
            let mut found: Vec<(usize, [P2; 4])> = vec![];
            for t in &tags {
                if t.1 == [a, b][t.0 % 2] && !found.iter().any(|f| f.0 == t.0) {
                    found.push((t.0, t.2));
                }
            }
            if best.as_ref().is_none_or(|x| found.len() > x.1.len()) {
                best = Some(([a, b], found));
            }
        }
    }
    best
}

/// Page code and mm -> photo px homography, from the AprilTags in a photo of a sheet page.
/// Any 3 of the 4 markers are enough (their 12 corners fit the homography).
pub fn detect_page(gray: &Gray) -> Option<(Code, crate::geom::Homography)> {
    let mut best: Option<(Code, Vec<(usize, [P2; 4])>)> = None;
    for side in [2600usize, 4000, 1800] {
        if let Some(m) = match_code(&detect_tags(gray, side)) {
            if best.as_ref().is_none_or(|b| m.1.len() > b.1.len()) {
                best = Some(m);
            }
        }
        if best.as_ref().is_some_and(|b| b.1.len() == 4) {
            break;
        }
    }
    let (code, found) = best?;
    if found.len() < 3 {
        return None;
    }
    let mut src = vec![];
    let mut dst = vec![];
    for (slot, c) in &found {
        src.extend(marker_corners_mm(*slot));
        dst.extend(c.iter().copied());
    }
    Some((code, crate::geom::Homography::fit(&src, &dst)?))
}

/// Raster of a page's markers only (tests and previews), white background, at `ppm`.
pub fn marker_raster(code: Code, ppm: f64) -> Gray {
    let (w, h) = ((PAGE_W * ppm) as usize, (PAGE_H * ppm) as usize);
    let mut g = Gray::new(w, h, 1.0);
    for slot in 0..4 {
        let cells = tag_cells(tag_id(code, slot));
        let [mx, my] = MARKER_POS[slot];
        let c = MARKER / 10.0;
        for y in 0..(MARKER * ppm) as usize {
            for x in 0..(MARKER * ppm) as usize {
                let (cx, cy) = (((x as f64 / ppm) / c) as usize, ((y as f64 / ppm) / c) as usize);
                if cx < 10 && cy < 10 && cells[cy][cx] {
                    let (px, py) = ((mx * ppm) as usize + x, (my * ppm) as usize + y);
                    if px < w && py < h {
                        g.set(px, py, 0.0);
                    }
                }
            }
        }
    }
    g
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_sheet_unchanged_and_detected() {
        let spec = first_sheet();
        use PageKind::*;
        assert_eq!(spec.iter().map(|p| p.kind).collect::<Vec<_>>(), [Latin, Latin, Cyrillic, Cyrillic, Digits, Math, Words, Words]);
        assert_eq!((spec[0].items.len(), spec[6].items.len(), spec[7].items.len()), (156, 78, 15));
        assert_eq!(spec[7].items[14].bl, 27.0 + 11.0 + 14.0 * 16.0);
        let g = marker_raster([3, 3], 6.0);
        let (code, h) = detect_page(&g).expect("markers");
        assert_eq!(code, [3, 3]);
        assert_eq!(resolve(code).unwrap().index, 3);
        let c = h.apply(marker_center_mm(2));
        assert!((c[0] - (188.0 + 7.0) * 6.0).abs() < 3.0 && (c[1] - (275.0 + 7.0) * 6.0).abs() < 3.0);
    }

    #[test]
    fn recipes_round_trip() {
        let r = Recipe { langs: vec![Lang::UzLatn, Lang::Ru], level: Level::Normal };
        assert_eq!(Recipe::from_id(r.id()).unwrap(), r);
        let pages = recipe_pages(&r);
        let kinds: Vec<PageKind> = pages.iter().map(|p| p.kind).collect();
        assert!(kinds.contains(&PageKind::Latin) && kinds.contains(&PageKind::Cyrillic) && kinds.contains(&PageKind::Digits));
        assert_eq!(pages.iter().filter(|p| p.kind == PageKind::Words).count(), 2);
        // a recipe page found from its markers
        let last = pages.last().unwrap();
        let (code, _) = detect_page(&marker_raster(last.code, 6.0)).expect("markers");
        assert_eq!(code, last.code);
        assert_eq!(resolve(code).unwrap().items.len(), last.items.len());
        // stored pages are not resolved here
        let s = stored_code(1459);
        assert!(is_stored(s) && resolve(s).is_none());
        assert_eq!(detect_page(&marker_raster(s, 6.0)).unwrap().0, s);
        // quick is shorter than best
        let q = recipe_pages(&Recipe { level: Level::Quick, ..r.clone() }).len();
        let b = recipe_pages(&Recipe { level: Level::Best, ..r }).len();
        assert!(q < b, "{q} {b}");
    }
}
