//! A "hand": real ink glyphs from one person's writing, their measurements and repair edits.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};

use crate::charset::{self, Lang};
use crate::img::Gray;
use crate::signal;
use crate::skeleton;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Src {
    Box,
    Text,
    /// made by the app for a character the person never wrote (see `synth`)
    Auto,
}

/// How a made-up glyph was made.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum How {
    /// the other case of the same shape, resized
    Case,
    /// a written letter plus a drawn mark
    Mark,
    /// two written letters side by side
    Join,
    /// a simple symbol drawn with the person's pen
    Draw,
    /// a font's letter shape redrawn with the person's pen
    Font,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Flag {
    /// ink touches the edge of its box: probably cut off
    Cut,
    /// very light ink
    Faint,
    /// size far from the person's normal letter size
    Size,
    /// almost identical to another variant
    Duplicate,
    /// looks unlike the other versions of its character (probably cut wrongly from a page)
    Odd,
}

/// Where a glyph cut from running writing stood: the written word it came from (an id, not
/// the text), its place in that word, its neighbours, and its position on the line.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Ctx {
    pub word: u64,
    pub pos: u16,
    pub prev: Option<char>,
    pub next: Option<char>,
    /// left edge of the glyph image and the baseline, in px of the written line
    pub x: f32,
    pub y: f32,
}

/// Gaps between neighbouring words of the person's running writing, in x-heights of their
/// page, by page: from where each glyph stood. Not running writing: hand-cut sheets of single
/// signs (lines 1000 and up) and pages of characters written apart (words of one glyph).
pub fn word_gap_samples(hand: &Hand) -> std::collections::BTreeMap<u64, Vec<f32>> {
    // each word: ink from left to right on its line, px per x-height there, glyphs in it
    let mut words: std::collections::BTreeMap<u64, (f32, f32, f32, usize)> = Default::default();
    for g in hand.glyphs.iter().filter(|g| g.src != Src::Auto) {
        let Some(c) = &g.ctx else { continue };
        if (c.word >> 16) & 0xFFFF >= 1000 {
            continue;
        }
        let pxh = g.ppm * hand.page_scale(g) * hand.meta.xh_mm;
        let e = words.entry(c.word).or_insert((f32::MAX, f32::MIN, pxh, 0));
        e.0 = e.0.min(c.x + g.ink_x[0]);
        e.1 = e.1.max(c.x + g.ink_x[1]);
        e.3 += g.text.chars().count();
    }
    let mut per_page: std::collections::BTreeMap<u64, Vec<f32>> = Default::default();
    for (&id, w) in &words {
        per_page.entry(id >> 32).or_default().push(w.3 as f32);
    }
    let running = |page: u64| per_page.get(&page).is_some_and(|n| signal::median(n) >= 2.5);
    let mut out: std::collections::BTreeMap<u64, Vec<f32>> = Default::default();
    for (&id, &(_, right, pxh, _)) in &words {
        if !running(id >> 32) {
            continue;
        }
        if let Some(&(left, _, _, _)) = words.get(&(id + 1)) {
            let gap = (left - right) / pxh.max(1e-3);
            // overlapping or a line apart: not two neighbouring words
            if gap > 0.05 && gap < 6.0 {
                out.entry(id >> 32).or_default().push(gap);
            }
        }
    }
    out
}

/// Non-destructive repair steps, replayed on the original matte.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum Edit {
    Erase { pts: Vec<[f32; 2]>, r: f32 },
    Restore { pts: Vec<[f32; 2]>, r: f32 },
    Rotate { deg: f32 },
    Slant { k: f32 },
    Scale { f: f32 },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Glyph {
    pub id: String,
    pub text: String,
    pub src: Src,
    pub w: u32,
    pub h: u32,
    /// baseline, pixels from the top of the image
    pub bl: f32,
    /// pixels per millimetre of the source photo
    pub ppm: f32,
    pub on: bool,
    /// stroke width, px
    pub sw: f32,
    /// centre lines for pen plotters, px
    pub skel: Vec<Vec<[f32; 2]>>,
    /// [x, y, dx, dy]: where a join may arrive / leave, with stroke direction
    pub entry: Option<[f32; 4]>,
    pub exit: Option<[f32; 4]>,
    /// leftmost / rightmost ink column, px
    pub ink_x: [f32; 2],
    #[serde(default)]
    pub flags: Vec<Flag>,
    #[serde(default)]
    pub edits: Vec<Edit>,
    /// set at import when ink touched the box edge
    #[serde(default)]
    pub touched_edge: bool,
    /// set at import when it looked unlike the other versions
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub odd: bool,
    /// cut from joined writing: [entry, exit] where its joining strokes were cut ([x, y, dx, dy])
    #[serde(default, skip_serializing_if = "is_none2")]
    pub stubs: [Option<[f32; 4]>; 2],
    /// cut from running writing: where it stood
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ctx: Option<Ctx>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub how: Option<How>,
    /// characters a made-up glyph was built from
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub from: String,
    /// glyph ids it was built from (rebuilt when those go)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub base: Vec<String>,
    /// a print letter (letters apart, shaped like a printed T) rather than the person's
    /// joined handwriting; only letters have a style, digits and marks serve both
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub print: bool,
}

/// A version holds a letter (so it has a style: joined or print).
pub fn has_letter(text: &str) -> bool {
    text.chars().any(charset::is_letter)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Stats {
    /// letter gap in x-heights (mean, sd)
    pub gap: [f32; 2],
    /// word gap in x-heights
    pub word: [f32; 2],
    /// share of letter pairs written joined
    pub join: f32,
    /// "ab" -> [joined, seen]
    pub pairs: HashMap<String, [u32; 2]>,
}

impl Default for Stats {
    fn default() -> Self {
        Stats { gap: [0.45, 0.15], word: [1.25, 0.25], join: 0.35, pairs: HashMap::new() }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HandMeta {
    pub id: String,
    pub name: String,
    pub created: f64,
    /// ink reflectance relative to paper (sRGB ratio)
    pub ink: [f32; 3],
    pub ink_n: f32,
    pub xh_mm: f32,
    pub box_scale: f32,
    pub stats: Stats,
    #[serde(default)]
    pub imports: Vec<serde_json::Value>,
    /// what the person writes in (empty = everything)
    #[serde(default)]
    pub langs: Vec<Lang>,
    /// measured slant of upright strokes: x shift per unit of height (positive leans right)
    #[serde(default)]
    pub lean: f32,
    /// capital height / x-height
    #[serde(default = "default_cap")]
    pub cap: f32,
    /// x-height (mm) of each imported page, measured on its own letters (key: the import id,
    /// the high bits of its words' ids). A page written bigger, or whose scale was misread,
    /// is brought to the person's usual size.
    #[serde(default)]
    pub page_xh: HashMap<u64, f32>,
}

fn is_none2(v: &[Option<[f32; 4]>; 2]) -> bool {
    v[0].is_none() && v[1].is_none()
}

fn default_cap() -> f32 {
    1.45
}

impl HandMeta {
    pub fn new(id: &str, name: &str) -> Self {
        HandMeta {
            id: id.into(),
            name: name.into(),
            created: now(),
            ink: [0.15, 0.17, 0.38],
            ink_n: 0.0,
            xh_mm: 3.0,
            box_scale: 1.0,
            stats: Stats::default(),
            imports: vec![],
            langs: vec![],
            lean: 0.0,
            cap: default_cap(),
            page_xh: HashMap::new(),
        }
    }
}

pub fn now() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..12].to_string()
}

/// In-memory hand. Glyph images live in `dir` as `<id>.png` (original matte).
pub struct Hand {
    pub meta: HandMeta,
    pub glyphs: Vec<Glyph>,
    pub dir: PathBuf,
    cache: RwLock<HashMap<String, Arc<Gray>>>,
    /// characters that could not be made up (not retried in this session)
    pub unmakeable: Mutex<HashSet<char>>,
}

impl Hand {
    pub fn new(meta: HandMeta, glyphs: Vec<Glyph>, dir: PathBuf) -> Self {
        Hand { meta, glyphs, dir, cache: RwLock::new(HashMap::new()), unmakeable: Mutex::new(HashSet::new()) }
    }

    pub fn png_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.png"))
    }

    pub fn original(&self, id: &str) -> anyhow::Result<Gray> {
        let img = image::open(self.png_path(id))?.to_luma8();
        Ok(Gray::from_u8(&img))
    }

    /// Current matte (original + edits), cached.
    pub fn alpha(&self, id: &str) -> Arc<Gray> {
        if let Some(a) = self.cache.read().get(id) {
            return a.clone();
        }
        let g = self.glyphs.iter().find(|g| g.id == id);
        let orig = self.original(id).unwrap_or_else(|_| Gray::new(1, 1, 0.0));
        let a = Arc::new(match g {
            Some(g) if !g.edits.is_empty() => apply_edits(&orig, &g.edits),
            _ => orig,
        });
        self.cache.write().insert(id.to_string(), a.clone());
        a
    }

    pub fn invalidate(&self, id: &str) {
        self.cache.write().remove(id);
    }

    pub fn get(&self, id: &str) -> Option<&Glyph> {
        self.glyphs.iter().find(|g| g.id == id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Glyph> {
        self.glyphs.iter_mut().find(|g| g.id == id)
    }

    /// Save a new glyph matte and measure it.
    pub fn add_glyph(&mut self, text: &str, alpha: &Gray, bl: f32, ppm: f32, src: Src, touched_edge: bool) -> anyhow::Result<String> {
        let id = new_id();
        std::fs::create_dir_all(&self.dir)?;
        alpha.to_u8().save(self.png_path(&id))?;
        let f = features(alpha);
        self.glyphs.push(Glyph {
            id: id.clone(),
            text: text.into(),
            src,
            w: alpha.w as u32,
            h: alpha.h as u32,
            bl,
            ppm,
            on: true,
            sw: f.sw,
            skel: f.skel,
            entry: None,
            exit: None,
            ink_x: f.ink_x,
            flags: vec![],
            edits: vec![],
            touched_edge,
            odd: false,
            stubs: [None, None],
            ctx: None,
            how: None,
            from: String::new(),
            base: vec![],
            print: false,
        });
        self.cache.write().insert(id.clone(), Arc::new(alpha.clone()));
        Ok(id)
    }

    /// How much bigger than the person's usual size this version's page came out (1 = usual).
    pub fn page_scale(&self, g: &Glyph) -> f32 {
        g.ctx
            .as_ref()
            .and_then(|c| self.meta.page_xh.get(&(c.word >> 32)))
            .map_or(1.0, |x| (x / self.meta.xh_mm.max(0.1)).clamp(0.4, 2.5))
    }

    pub fn remove_glyph(&mut self, id: &str) {
        self.glyphs.retain(|g| g.id != id);
        let _ = std::fs::remove_file(self.png_path(id));
        self.invalidate(id);
    }

    /// Versions cut by earlier versions of the app as a letter with the tutuq belgisi stuck to
    /// it ("a'" in taʼlim): the apostrophe is a sign of its own, so it is cut out into a
    /// version of `charset::TUTUQ` and the letter kept alone; without a mark to cut out, the
    /// version goes. Returns how many were changed.
    pub fn split_apostrophes(&mut self) -> usize {
        const APOS: &[char] = &['\'', '’', charset::TUTUQ];
        let stuck: Vec<Glyph> = self
            .glyphs
            .iter()
            .filter(|g| {
                let cs: Vec<char> = g.text.chars().collect();
                cs.len() > 1 && cs.iter().any(|c| APOS.contains(c)) && cs.iter().any(|c| c.is_alphanumeric())
            })
            .cloned()
            .collect();
        // how much ink the letters' other versions usually hold (to leave out blobs)
        let mut usual: HashMap<String, f32> = HashMap::new();
        for g in &stuck {
            let letters: String = g.text.chars().filter(|c| !APOS.contains(c)).collect();
            if usual.contains_key(&letters) {
                continue;
            }
            let v: Vec<f32> = self
                .glyphs
                .iter()
                .filter(|o| o.text == letters && o.on)
                .take(60)
                .map(|o| ink_area(&self.alpha(&o.id), self.meta.xh_mm * o.ppm * self.page_scale(o)))
                .collect();
            if v.len() >= 8 {
                usual.insert(letters, signal::median(&v));
            }
        }
        for g in &stuck {
            let a = self.alpha(&g.id);
            let xh = self.meta.xh_mm * g.ppm * self.page_scale(g);
            let cs: Vec<char> = g.text.chars().collect();
            let at_end = APOS.contains(cs.last().unwrap());
            let letters: String = cs.iter().filter(|c| !APOS.contains(c)).collect();
            // the mark: a small piece wholly above the letters, on the apostrophe's side
            let mask: Vec<bool> = a.d.iter().map(|v| *v > 0.4).collect();
            let (lab, st) = crate::img::components(&mask, a.w, a.h);
            let mark = (1..st.len())
                .filter(|&i| {
                    let c = st[i];
                    (c.y1 as f32) < g.bl - 0.6 * xh && (c.area as f32) < 0.35 * xh * xh && (c.h() as f32) < 0.9 * xh
                })
                .max_by(|&i, &j| {
                    let (x, y) = (st[i].cx, st[j].cx);
                    if at_end { x.partial_cmp(&y).unwrap() } else { y.partial_cmp(&x).unwrap() }
                });
            self.remove_glyph(&g.id);
            let Some(m) = mark else { continue };
            let c = st[m];
            // the letter without the mark (and the faint edge around it)
            let mut rest = (*a).clone();
            for y in c.y0.saturating_sub(1)..(c.y1 + 1).min(a.h) {
                for x in c.x0.saturating_sub(1)..(c.x1 + 1).min(a.w) {
                    let l = lab[y * a.w + x];
                    if l == m as u32 || l == 0 {
                        rest.d[y * a.w + x] = 0.0;
                    }
                }
            }
            let pad = (0.6 * g.ppm) as usize;
            let (x0, y0) = (c.x0.saturating_sub(pad), c.y0.saturating_sub(pad));
            let mut alone = a.crop(x0, y0, c.x1 + pad, c.y1 + pad);
            for y in 0..alone.h {
                for x in 0..alone.w {
                    if lab.get((y0 + y) * a.w + x0 + x).is_some_and(|&l| l != m as u32 && l != 0) {
                        alone.d[y * alone.w + x] = 0.0;
                    }
                }
            }
            let ctx = g.ctx.clone();
            let n = letters.chars().count() as u16;
            let blob = unusable(features(&rest).sw, xh) || usual.get(&letters).is_some_and(|m| ink_area(&rest, xh) > HEAVY * m);
            let (lp, mp) = if at_end { (0, n) } else { (1, 0) };
            let Ok(mi) = self.add_glyph(&charset::TUTUQ.to_string(), &alone, g.bl - y0 as f32, g.ppm, g.src, false) else { continue };
            let li = if blob { None } else { self.add_glyph(&letters, &rest, g.bl, g.ppm, g.src, g.touched_edge).ok() };
            if let Some(l) = li.and_then(|li| self.get_mut(&li)) {
                l.on = g.on;
                l.odd = g.odd;
                l.print = g.print;
                l.stubs = g.stubs;
                l.ctx = ctx.clone().map(|x| Ctx {
                    pos: x.pos + lp,
                    prev: if at_end { x.prev } else { Some(charset::TUTUQ) },
                    next: if at_end { Some(charset::TUTUQ) } else { x.next },
                    ..x
                });
            }
            if let Some(t) = self.get_mut(&mi) {
                t.on = g.on;
                t.ctx = ctx.map(|x| Ctx {
                    pos: x.pos + mp,
                    prev: if at_end { cs.iter().rev().nth(1).copied() } else { x.prev },
                    next: if at_end { x.next } else { cs.get(1).copied() },
                    x: x.x + x0 as f32,
                    ..x
                });
            }
        }
        // an apostrophe cut on its own between two letters is the tutuq belgisi too
        let mut alone = 0;
        for g in self.glyphs.iter_mut().filter(|g| g.text == "'" || g.text == "’") {
            if g.ctx.as_ref().is_some_and(|c| c.prev.is_some_and(char::is_alphabetic) && c.next.is_some_and(char::is_alphabetic)) {
                g.text = charset::TUTUQ.to_string();
                alone += 1;
            }
        }
        if !stuck.is_empty() || alone > 0 {
            self.refresh();
        }
        stuck.len() + alone
    }

    /// Enabled glyphs by bank key.
    pub fn bank(&self) -> HashMap<String, Vec<&Glyph>> {
        let mut m: HashMap<String, Vec<&Glyph>> = HashMap::new();
        for g in self.glyphs.iter().filter(|g| g.on) {
            m.entry(g.text.clone()).or_default().push(g);
        }
        m
    }

    /// Re-measure x-height, box scale, join points and quality flags after any change.
    /// The person's word gaps, again from where every glyph stood in their running writing
    /// (median and spread, x-heights); None with too few. Each import used to set them from its
    /// own page alone, so a page of single signs (letters far apart) could make every gap huge.
    fn word_gaps(&self) -> Option<[f32; 2]> {
        let g: Vec<f32> = word_gap_samples(self).into_values().flatten().collect();
        (g.len() >= 8).then(|| [signal::median(&g), signal::mad_sigma(&g).clamp(0.1, 0.4)])
    }

    pub fn refresh(&mut self) {
        let mut hs_box = Vec::new();
        let mut hs_text = Vec::new();
        let mut by_page: HashMap<u64, Vec<f32>> = HashMap::new();
        for g in self.glyphs.iter().filter(|g| g.on && g.src != Src::Auto) {
            let mut cs = g.text.chars();
            let (Some(c), None) = (cs.next(), cs.next()) else { continue };
            if !charset::X_HEIGHT.contains(&c) {
                continue;
            }
            let a = self.alpha(&g.id);
            if let Some(top) = (0..a.h).find(|&y| (0..a.w).any(|x| a.at(x, y) > 0.3)) {
                let v = (g.bl - top as f32) / g.ppm;
                if g.src == Src::Box { hs_box.push(v) } else { hs_text.push(v) }
                if let Some(c) = &g.ctx {
                    by_page.entry(c.word >> 32).or_default().push(v);
                }
            }
        }
        let bx = (!hs_box.is_empty()).then(|| signal::median(&hs_box));
        let tx = (hs_text.len() >= 5).then(|| signal::median(&hs_text));
        self.meta.page_xh = by_page.into_iter().filter(|(_, v)| v.len() >= 12).map(|(k, v)| (k, signal::median(&v))).collect();
        // the usual size: one say per page, so a page whose scale was misread cannot shift it
        let pages: Vec<f32> = self.meta.page_xh.values().copied().collect();
        let tx = if pages.len() >= 3 { Some(signal::median(&pages)) } else { tx };
        let xh = tx.or(bx).unwrap_or(self.meta.xh_mm);
        self.meta.xh_mm = xh;
        // People write bigger in boxes than in running text; scale boxes to match.
        self.meta.box_scale = match (bx, tx) {
            (Some(b), Some(t)) => (t / b).clamp(0.6, 1.2),
            _ => 1.0,
        };
        if let Some(w) = self.word_gaps() {
            self.meta.stats.word = w;
        }
        let ids: Vec<String> = self.glyphs.iter().map(|g| g.id.clone()).collect();
        let mut thumbs: HashMap<String, Vec<(String, Vec<f32>)>> = HashMap::new();
        let (mut caps, mut leans) = (vec![], vec![]);
        for id in ids {
            let a = self.alpha(&id);
            let g = self.get(&id).unwrap().clone();
            let src_xh = if g.src == Src::Box { bx.unwrap_or(xh) } else { xh } * g.ppm * self.page_scale(&g);
            let f = if g.edits.is_empty() { None } else { Some(features(&a)) };
            let (entry, exit) = join_points(&a, g.bl, src_xh, f.as_ref().map(|f| &f.skel).unwrap_or(&g.skel));
            let mut flags = Vec::new();
            if g.touched_edge {
                flags.push(Flag::Cut);
            }
            if g.odd {
                flags.push(Flag::Odd);
            }
            let ink: Vec<f32> = a.d.iter().copied().filter(|&v| v > 0.2).collect();
            if !ink.is_empty() && signal::median(&ink) < 0.45 {
                flags.push(Flag::Faint);
            }
            let top = (0..a.h).find(|&y| (0..a.w).any(|x| a.at(x, y) > 0.3)).unwrap_or(0) as f32;
            let bot = (0..a.h).rev().find(|&y| (0..a.w).any(|x| a.at(x, y) > 0.3)).unwrap_or(a.h) as f32;
            let above = (g.bl - top) / src_xh.max(1.0);
            let total = (bot - top) / src_xh.max(1.0);
            if g.on && g.src != Src::Auto {
                let mut cs = g.text.chars();
                if let (Some(c), None) = (cs.next(), cs.next()) {
                    if CAPS.contains(c) {
                        caps.push(above);
                    }
                    if charset::is_letter(c) {
                        leans.extend(stroke_leans(f.as_ref().map(|f| &f.skel).unwrap_or(&g.skel), 0.45 * src_xh));
                    }
                }
            }
            let single_x = g.text.chars().count() == 1 && charset::X_HEIGHT.contains(&g.text.chars().next().unwrap());
            if (single_x && !(0.6..=1.6).contains(&above)) || total > 4.5 || (g.text.chars().all(char::is_alphanumeric) && total < 0.25) {
                flags.push(Flag::Size);
            }
            if g.src != Src::Auto {
                thumbs.entry(g.text.clone()).or_default().push((id.clone(), thumb16(&a)));
            }
            let gm = self.get_mut(&id).unwrap();
            // joins found on the page beat guesses from the shape (unless the letter was edited)
            let stubs = if g.edits.is_empty() { g.stubs } else { [None, None] };
            gm.entry = stubs[0].or(entry);
            gm.exit = stubs[1].or(exit);
            gm.flags = flags;
            if let Some(f) = f {
                gm.sw = f.sw;
                gm.skel = f.skel;
                gm.ink_x = f.ink_x;
                gm.w = a.w as u32;
                gm.h = a.h as u32;
            }
        }
        if caps.len() >= 4 {
            self.meta.cap = signal::median(&caps).clamp(1.15, 2.0);
        }
        if leans.len() >= 15 {
            self.meta.lean = signal::median(&leans).clamp(-0.35, 0.6);
        }
        for list in thumbs.values() {
            for i in 0..list.len() {
                for j in 0..i {
                    // only near pixel-identical copies: real handwriting varies far more
                    if corr(&list[i].1, &list[j].1) > 0.993 {
                        if let Some(g) = self.get_mut(&list[i].0) {
                            if !g.flags.contains(&Flag::Duplicate) {
                                g.flags.push(Flag::Duplicate);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Per group of the person's languages: every character with how many written versions
    /// (aliases count for both scripts, and marks written the same: ' and ʼ). Letters come
    /// twice: joined as the person writes ("latin") and print ("latin_print").
    pub fn coverage(&self) -> Vec<CovGroup> {
        let (mut joined, mut print): (HashMap<&str, usize>, HashMap<&str, usize>) = Default::default();
        for g in self.glyphs.iter().filter(|g| g.on && g.src != Src::Auto) {
            *if g.print { &mut print } else { &mut joined }.entry(g.text.as_str()).or_insert(0) += 1;
        }
        let count = |ms: &[&HashMap<&str, usize>], c: char| -> usize {
            charset::candidates(c).iter().map(|k| ms.iter().map(|m| m.get(k.to_string().as_str()).copied().unwrap_or(0)).sum::<usize>()).sum()
        };
        let mut out = vec![];
        for (group, chars) in charset::target_chars(&self.meta.langs) {
            let letters = group == "latin" || group == "cyrillic";
            let ms: &[&HashMap<&str, usize>] = if letters { &[&joined] } else { &[&joined, &print] };
            out.push(CovGroup { group: group.to_string(), chars: chars.iter().map(|&c| CovChar { c, n: count(ms, c) }).collect() });
            if letters {
                out.push(CovGroup { group: format!("{group}_print"), chars: chars.iter().map(|&c| CovChar { c, n: count(&[&print], c) }).collect() });
            }
        }
        out
    }

    /// How many characters of the person's languages have no written version yet (letters
    /// joined and print apart).
    pub fn not_written(&self) -> usize {
        self.coverage().iter().map(|g| g.chars.iter().filter(|c| c.n == 0).count()).sum()
    }

    /// Characters to ask for again, with how many boxes: missing ones, and ones whose only
    /// versions are flagged (cut off, faint, odd size).
    pub fn refill_list(&self) -> Vec<(String, usize)> {
        let mut good: HashMap<String, usize> = HashMap::new();
        for g in self.glyphs.iter().filter(|g| g.on && g.src != Src::Auto) {
            if !g.flags.iter().any(|f| matches!(f, Flag::Cut | Flag::Faint | Flag::Size)) {
                *good.entry(g.text.clone()).or_insert(0) += 1;
            }
        }
        let mut out = vec![];
        let mut seen = HashSet::new();
        for (_, chars) in charset::target_chars(&self.meta.langs) {
            for c in chars {
                // an aliased letter is asked for once, as its Latin twin
                let key = charset::alias_of(c).unwrap_or(c);
                if !seen.insert(key) {
                    continue;
                }
                let have: usize = charset::same(key).iter().map(|k| good.get(&k.to_string()).copied().unwrap_or(0)).sum();
                let want = if (key.is_lowercase() && charset::is_letter(key)) || key.is_ascii_digit() { 2 } else { 1 };
                if have < want {
                    out.push((key.to_string(), want - have));
                }
            }
        }
        out
    }
}

/// Uppercase letters without descenders or accents, for the capital height.
const CAPS: &str = "ABDEFHIKLMNPRTUZБВГЕЗИКЛМНПРТЭЯ";

/// Slants (x shift per unit of height, positive = right) of the straight, mostly upright
/// stretches of a glyph's centre lines that are at least `min_h` px tall.
fn stroke_leans(skel: &[Vec<[f32; 2]>], min_h: f32) -> Vec<f32> {
    let mut out = vec![];
    for p in skel {
        let mut i = 0;
        while i + 1 < p.len() {
            let Some(j) = (i + 1..p.len()).find(|&j| (p[j][1] - p[i][1]).abs() >= min_h) else { break };
            let (dx, dy) = (p[j][0] - p[i][0], p[j][1] - p[i][1]);
            let path: f32 = (i..j).map(|k| ((p[k + 1][0] - p[k][0]).powi(2) + (p[k + 1][1] - p[k][1]).powi(2)).sqrt()).sum();
            let chord = (dx * dx + dy * dy).sqrt();
            if chord / path.max(1e-6) > 0.92 && dx.abs() < 0.7 * dy.abs() {
                out.push(-dx / dy);
            }
            i = j;
        }
    }
    out
}

#[derive(Serialize, Clone, Debug)]
pub struct CovChar {
    pub c: char,
    /// written versions
    pub n: usize,
}

#[derive(Serialize, Clone, Debug)]
pub struct CovGroup {
    pub group: String,
    pub chars: Vec<CovChar>,
}

fn thumb16(a: &Gray) -> Vec<f32> {
    let s = a.resize(16, 16);
    let m = signal::mean(&s.d);
    s.d.iter().map(|v| v - m).collect()
}

fn corr(a: &[f32], b: &[f32]) -> f32 {
    let ab: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let aa: f32 = a.iter().map(|x| x * x).sum();
    let bb: f32 = b.iter().map(|x| x * x).sum();
    ab / (aa * bb).sqrt().max(1e-9)
}

pub struct Features {
    pub sw: f32,
    pub skel: Vec<Vec<[f32; 2]>>,
    pub ink_x: [f32; 2],
}

pub fn features(a: &Gray) -> Features {
    let mask: Vec<bool> = a.d.iter().map(|&v| v > 0.4).collect();
    let cols: Vec<usize> = (0..a.w).filter(|&x| (0..a.h).any(|y| a.at(x, y) > 0.25)).collect();
    let ink_x = if cols.is_empty() { [0.0, a.w as f32] } else { [cols[0] as f32, *cols.last().unwrap() as f32] };
    if mask.iter().filter(|&&b| b).count() < 3 {
        return Features { sw: 2.0, skel: vec![], ink_x };
    }
    // distance to background for every ink pixel
    let bg = image::GrayImage::from_raw(a.w as u32, a.h as u32, mask.iter().map(|&m| if m { 0 } else { 255 }).collect()).unwrap();
    let dt = imageproc::distance_transform::euclidean_squared_distance_transform(&bg);
    let sk = skeleton::thin(&mask, a.w, a.h);
    let d: Vec<f32> = (0..a.w * a.h).filter(|&i| sk[i]).map(|i| (dt.as_raw()[i] as f32).sqrt()).collect();
    let sw = if d.is_empty() { 2.0 } else { (2.0 * signal::median(&d)).max(1.0) };
    Features { sw, skel: skeleton::trace(&sk, a.w, a.h), ink_x }
}

/// How much ink a version holds, in x-heights squared: one with far more than the others of
/// its character is ink run together (filled loops; open loops are normal handwriting).
pub fn ink_area(a: &Gray, xh_px: f32) -> f32 {
    a.d.iter().filter(|v| **v > 0.4).count() as f32 / (xh_px * xh_px).max(1.0)
}

/// A letter that cannot be used: ink run together on a blurry photo, strokes nearly half as
/// thick as the x-height (`sw`: its stroke width, px), or `HEAVY` times the usual ink.
pub fn unusable(sw: f32, xh_px: f32) -> bool {
    sw > 0.45 * xh_px
}

pub const HEAVY: f32 = 1.8;

/// Where a join may leave (exit) and arrive (entry): free stroke ends in the lower part of the
/// letter, with the stroke direction. Letters without such ends are never joined.
pub fn join_points(a: &Gray, bl: f32, xh_px: f32, skel: &[Vec<[f32; 2]>]) -> (Option<[f32; 4]>, Option<[f32; 4]>) {
    let mut ends: Vec<([f32; 2], [f32; 2])> = Vec::new();
    for p in skel.iter().filter(|p| p.len() >= 2) {
        let n = p.len();
        let k0 = 2.min(n - 1);
        ends.push((p[0], [p[0][0] - p[k0][0], p[0][1] - p[k0][1]]));
        let k1 = n.saturating_sub(3).min(n - 1);
        ends.push((p[n - 1], [p[n - 1][0] - p[k1][0], p[n - 1][1] - p[k1][1]]));
    }
    let free: Vec<([f32; 2], [f32; 2])> = ends
        .iter()
        .enumerate()
        .filter(|(i, (pt, _))| ends.iter().enumerate().all(|(j, (q, _))| j == *i || ((pt[0] - q[0]).powi(2) + (pt[1] - q[1]).powi(2)).sqrt() > 2.0))
        .filter_map(|(_, (pt, d))| {
            let n = (d[0] * d[0] + d[1] * d[1]).sqrt();
            (n > 0.0).then(|| (*pt, [d[0] / n, d[1] / n]))
        })
        .collect();
    let (lo, hi) = (bl - 0.55 * xh_px, bl + 0.15 * xh_px);
    let w = a.w as f32;
    let low: Vec<_> = free.iter().filter(|(p, _)| p[1] >= lo && p[1] <= hi).collect();
    let exit = low.iter().filter(|(p, d)| p[0] > 0.55 * w && d[0] > -0.3).max_by(|a, b| a.0[0].partial_cmp(&b.0[0]).unwrap());
    let entry = low.iter().filter(|(p, d)| p[0] < 0.45 * w && -d[0] > -0.3).min_by(|a, b| a.0[0].partial_cmp(&b.0[0]).unwrap());
    (
        entry.map(|(p, d)| [p[0], p[1], -d[0], -d[1]]),
        exit.map(|(p, d)| [p[0], p[1], d[0], d[1]]),
    )
}

/// Apply repair edits to an original matte (canvas size never changes, so baselines hold).
/// Restore brings back ink from the matte as transformed so far (not the raw original), so
/// brushing after a rotate/slant/scale works on what the user sees.
pub fn apply_edits(orig: &Gray, edits: &[Edit]) -> Gray {
    let mut a = orig.clone();
    let mut base = orig.clone();
    for e in edits {
        if let Edit::Rotate { deg } = e {
            base = affine(&base, deg.to_radians(), 0.0, 1.0);
        } else if let Edit::Slant { k } = e {
            base = affine(&base, 0.0, *k, 1.0);
        } else if let Edit::Scale { f } = e {
            base = affine(&base, 0.0, 0.0, *f);
        }
        match e {
            Edit::Erase { pts, r } | Edit::Restore { pts, r } => {
                let restore = matches!(e, Edit::Restore { .. });
                for p in pts {
                    let (x0, x1) = ((p[0] - r).floor().max(0.0) as usize, ((p[0] + r).ceil() as usize).min(a.w));
                    let (y0, y1) = ((p[1] - r).floor().max(0.0) as usize, ((p[1] + r).ceil() as usize).min(a.h));
                    for y in y0..y1 {
                        for x in x0..x1 {
                            let d = ((x as f32 - p[0]).powi(2) + (y as f32 - p[1]).powi(2)).sqrt();
                            if d <= *r {
                                let soft = ((r - d) / 1.5).clamp(0.0, 1.0);
                                let i = y * a.w + x;
                                if restore {
                                    a.d[i] += (base.d[i] - a.d[i]) * soft;
                                } else {
                                    a.d[i] *= 1.0 - soft;
                                }
                            }
                        }
                    }
                }
            }
            Edit::Rotate { deg } => a = affine(&a, deg.to_radians(), 0.0, 1.0),
            Edit::Slant { k } => a = affine(&a, 0.0, *k, 1.0),
            Edit::Scale { f } => a = affine(&a, 0.0, 0.0, *f),
        }
    }
    a
}

/// Rotate / shear / scale about the image centre, same canvas size.
fn affine(a: &Gray, rot: f32, shear: f32, scale: f32) -> Gray {
    let (cx, cy) = (a.w as f32 / 2.0, a.h as f32 / 2.0);
    let (c, s) = (rot.cos(), rot.sin());
    let mut out = Gray::new(a.w, a.h, 0.0);
    for y in 0..a.h {
        for x in 0..a.w {
            // inverse map: output -> source
            let (dx, dy) = ((x as f32 - cx) / scale, (y as f32 - cy) / scale);
            let (rx, ry) = (c * dx + s * dy, -s * dx + c * dy);
            let sx = rx + shear * ry;
            out.set(x, y, a.sample(sx + cx, ry + cy));
        }
    }
    out
}

pub fn hand_dir(root: &Path, hid: &str) -> PathBuf {
    root.join("hands").join(hid)
}

/// A small picture of a version for comparing shapes: its ink from the left edge, scaled by
/// the person's x-height, with the baseline at a fixed row; blurred, mean taken out.
pub fn shape(a: &Gray, bl: f32, xh_px: f32) -> Vec<f32> {
    let (w, h, base) = (24usize, 34usize, 22.0f32);
    let k = 10.0 / xh_px.max(1.0);
    let cols: Vec<usize> = (0..a.w).filter(|&x| (0..a.h).any(|y| a.at(x, y) > 0.3)).collect();
    let x0 = cols.first().copied().unwrap_or(0) as f32;
    let mut t = vec![0f32; w * h];
    for (i, v) in t.iter_mut().enumerate() {
        let (tx, ty) = ((i % w) as f32, (i / w) as f32);
        let (sx, sy) = (x0 + (tx - 1.0) / k, bl + (ty - base) / k);
        *v = if sx < 0.0 || sy < 0.0 || sx > a.w as f32 - 1.0 || sy > a.h as f32 - 1.0 { 0.0 } else { a.sample(sx, sy) };
    }
    let b = crate::img::blur(&Gray { w, h, d: t }, 1.2);
    let m = signal::mean(&b.d);
    b.d.iter().map(|v| v - m).collect()
}

/// How alike two shapes are (correlation, 1 = the same).
pub fn shape_sim(a: &[f32], b: &[f32]) -> f32 {
    let ab: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let aa: f32 = a.iter().map(|x| x * x).sum();
    let bb: f32 = b.iter().map(|x| x * x).sum();
    ab / (aa * bb).sqrt().max(1e-9)
}

/// For versions of one character: how much each looks like the most typical one (the version
/// most like all the others).
pub fn typicality(shapes: &[Vec<f32>]) -> Vec<f32> {
    if shapes.is_empty() {
        return vec![];
    }
    let score: Vec<f32> = shapes.iter().map(|a| signal::mean(&shapes.iter().map(|b| shape_sim(a, b)).collect::<Vec<_>>())).collect();
    let best = (0..shapes.len()).max_by(|&i, &j| score[i].partial_cmp(&score[j]).unwrap()).unwrap();
    shapes.iter().map(|a| shape_sim(a, &shapes[best])).collect()
}
