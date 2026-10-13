//! Lay document blocks out as handwriting: glyph instances + drawn strokes on numbered lines.
//!
//! Coordinates: x (mm from the writing start), l (line number on the page; fractional for
//! vertical strokes) and dy (mm below that line's baseline). The paper decides where line l
//! really is, so text follows the printed lines and the curvature of the sheet.

use std::collections::{HashMap, VecDeque};

use rand::{Rng, SeedableRng};
use rand_distr::StandardNormal;
use rand_pcg::Pcg64Mcg;
use serde::{Deserialize, Serialize};

use crate::charset;
use crate::hand::{Hand, Src};
use crate::ingest::{text::latex_to_text, Block, BOLD, BOLD_END, END, SUB, SUP, UNDER, UNDER_END};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Params {
    /// multiply natural writing size
    pub size: f32,
    /// amount of human variation (0 = very neat)
    pub mess: f32,
    /// multiply the person's join probability (0 = never join)
    pub joins: f32,
    pub indent_mm: f32,
    pub seed: u64,
    /// extra slant in degrees (positive leans right)
    pub slant: f32,
    pub letter_spacing: f32,
    pub word_spacing: f32,
    /// how much lines wander (1 = normal)
    pub line_drift: f32,
    /// 0 = slow & careful, 1 = fast: more slant, joins and drift
    pub speed: f32,
    /// crossed-out mistakes per 100 words
    pub mistakes: f32,
    /// "wordIdx:charIdx" -> glyph id
    pub overrides: HashMap<String, String>,
    /// joined handwriting or print letters
    pub letters: Letters,
}

/// Which letters to write with.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Letters {
    /// print letters for English (most people print it), joined writing for the rest; a
    /// letter without a version of that style takes the other
    #[default]
    Auto,
    Joined,
    /// print letters only: a letter never written in print is missing
    Print,
}

impl Default for Params {
    fn default() -> Self {
        Params {
            size: 1.0,
            mess: 1.0,
            joins: 1.0,
            indent_mm: 10.0,
            seed: 1,
            slant: 0.0,
            letter_spacing: 1.0,
            word_spacing: 1.0,
            line_drift: 1.0,
            speed: 0.3,
            mistakes: 0.0,
            overrides: HashMap::new(),
            letters: Letters::Auto,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Geometry {
    pub width_mm: f32,
    /// each line's own width, from the first line of the page (a bent writing area); else width_mm
    #[serde(default)]
    pub widths: Vec<f32>,
    pub lines_per_page: usize,
    pub pitch_mm: f32,
    /// printed lines: people follow them closely
    pub ruled: bool,
}

impl Default for Geometry {
    fn default() -> Self {
        Geometry { width_mm: 172.0, widths: vec![], lines_per_page: 32, pitch_mm: 8.0, ruled: true }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "k")]
pub enum Elem {
    /// glyph instance; anchor = glyph pixel (0, baseline) at (x, l, dy)
    #[serde(rename = "g")]
    G { gid: String, x: f32, l: f32, dy: f32, s: f32, r: f32, sh: f32, seed: u32, wi: u32, ci: u32, p: f32 },
    /// drawn pen stroke (joins, table lines, underlines, cross-outs); pts = [x, l, dy]
    #[serde(rename = "s")]
    S { pts: Vec<[f32; 3]>, w: Vec<f32>, p: f32 },
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct LayoutResult {
    pub pages: Vec<Vec<Elem>>,
    /// characters the person never wrote (left as a gap)
    pub missing: Vec<char>,
    /// letters to print the person wrote only joined (left as a gap)
    #[serde(default)]
    pub missing_print: Vec<char>,
    pub xh: f32,
    pub pen_mm: f32,
}

/// Glyph data the layout needs (copied so layout never borrows the hand).
#[derive(Clone, Debug)]
pub struct GInfo {
    pub id: String,
    pub text: String,
    pub src: Src,
    pub ppm: f32,
    pub bl: f32,
    pub w: f32,
    pub h: f32,
    pub entry: Option<[f32; 4]>,
    pub exit: Option<[f32; 4]>,
    pub ink_x: [f32; 2],
    pub sw: f32,
    /// [entry, exit] are the cut ends of real joining strokes (from joined writing)
    pub stub: [bool; 2],
    /// where it stood in running writing
    pub ctx: Option<crate::hand::Ctx>,
    /// a print letter
    pub print: bool,
}

/// One glyph to place in a word: its bank key, position, writing mode and neighbours.
#[derive(Clone, Debug)]
struct Unit {
    key: String,
    ci: u32,
    mode: Option<char>,
    prev: Option<char>,
    next: Option<char>,
    forced: Option<String>,
}

/// `b` was written right after `a`, in the same word.
fn adjacent(a: &GInfo, b: &GInfo) -> bool {
    match (&a.ctx, &b.ctx) {
        (Some(x), Some(y)) => x.word == y.word && y.pos as usize == x.pos as usize + a.text.chars().count(),
        _ => false,
    }
}

/// Glyph pixel (u, v) -> (x, dy) mm in line coordinates for a glyph element.
pub fn glyph_point(x: f32, dy: f32, s: f32, r: f32, sh: f32, bl: f32, u: f32, v: f32) -> (f32, f32) {
    let (mut du, dv) = (u * s, (v - bl) * s);
    du -= sh * dv;
    let (c, sn) = (r.to_radians().cos(), r.to_radians().sin());
    (x + c * du - sn * dv, dy + sn * du + c * dv)
}

pub struct Composer {
    pub p: Params,
    rng: Pcg64Mcg,
    bank: HashMap<String, Vec<GInfo>>,
    ligs: Vec<String>,
    recent: HashMap<String, VecDeque<String>>,
    /// written words used lately (their versions are avoided for a while)
    recent_words: VecDeque<u64>,
    pub xh: f32,
    gap: [f32; 2],
    wgap: [f32; 2],
    join_rate: f32,
    pairs: HashMap<String, [u32; 2]>,
    box_scale: f32,
    pub pen_mm: f32,
    pub missing: std::collections::BTreeSet<char>,
    pub missing_print: std::collections::BTreeSet<char>,
    /// the words now written are printed (letters apart, never joined)
    pub print: bool,
    pub fatigue: f32,
    pub size_mul: f32,
    pub pressure: f32,
    bold: bool,
    under: bool,
}

impl Composer {
    pub fn new(hand: &Hand, p: Params) -> Self {
        let mut bank: HashMap<String, Vec<GInfo>> = HashMap::new();
        for g in hand.glyphs.iter().filter(|g| g.on) {
            let gi = GInfo {
                id: g.id.clone(),
                text: g.text.clone(),
                src: g.src,
                // px per mm at the person's usual size (a page written bigger comes out usual)
                ppm: g.ppm * hand.page_scale(g),
                bl: g.bl,
                w: g.w as f32,
                h: g.h as f32,
                entry: g.entry,
                exit: g.exit,
                ink_x: g.ink_x,
                sw: g.sw,
                stub: [g.stubs[0].is_some() && g.edits.is_empty(), g.stubs[1].is_some() && g.edits.is_empty()],
                // an edited glyph no longer lines up with its neighbours on the page
                ctx: g.ctx.clone().filter(|_| g.edits.is_empty()),
                print: g.print,
            };
            bank.entry(g.text.clone()).or_default().push(gi.clone());
            // aliased shapes share variants (Latin a and Cyrillic а)
            let mut cs = g.text.chars();
            if let (Some(c), None) = (cs.next(), cs.next()) {
                for o in [charset::alias_of(c), charset::alias_back(c)].into_iter().flatten() {
                    bank.entry(o.to_string()).or_default().push(gi.clone());
                }
            }
        }
        let mut ligs: Vec<String> = bank.keys().filter(|k| k.chars().count() > 1).cloned().collect();
        ligs.sort_by_key(|k| std::cmp::Reverse(k.chars().count()));
        let st = &hand.meta.stats;
        let gap = st.gap;
        let w = st.word;
        let sws: Vec<f32> = hand.glyphs.iter().filter(|g| g.on && g.src != Src::Auto).map(|g| g.sw / (g.ppm * hand.page_scale(g))).collect();
        let speed = p.speed.clamp(0.0, 1.0);
        Composer {
            rng: Pcg64Mcg::seed_from_u64(p.seed),
            xh: hand.meta.xh_mm * p.size,
            gap: [gap[0] * p.letter_spacing, gap[1]],
            // a word gap must read as a word gap even when the person's spacing is tight
            wgap: [w[0].max(gap[0] + 2.5 * gap[1]).max(0.8) * p.word_spacing, w[1]],
            join_rate: (st.join + 0.15 * (speed - 0.3)).clamp(0.0, 1.0),
            pairs: st.pairs.clone(),
            box_scale: hand.meta.box_scale,
            pen_mm: if sws.is_empty() { 0.45 } else { crate::signal::median(&sws) },
            missing: Default::default(),
            missing_print: Default::default(),
            print: false,
            fatigue: 1.0,
            size_mul: 1.0,
            pressure: 1.0,
            bold: false,
            under: false,
            bank,
            ligs,
            recent: HashMap::new(),
            recent_words: VecDeque::new(),
            p,
        }
    }

    pub fn n(&mut self, sd: f32) -> f32 {
        let z: f32 = self.rng.sample(StandardNormal);
        z * sd * self.p.mess
    }

    fn lookup(&self, key: &str) -> Option<(String, &Vec<GInfo>)> {
        let mut cs = key.chars();
        if let (Some(c), None) = (cs.next(), cs.clone().next()) {
            // the character itself, or one shaped the same (a / а, quotes): never another letter
            for k in charset::candidates(c) {
                if let Some(v) = self.bank.get(&k.to_string()) {
                    return Some((k.to_string(), v));
                }
            }
            None
        } else {
            self.bank.get(key).map(|v| (key.to_string(), v))
        }
    }

    pub fn has(&self, key: &str) -> bool {
        self.lookup(key).is_some()
    }

    /// The versions of the style now written (joined or print) among `opts`; a letter without
    /// one takes the other style, except when only print letters were asked for. Digits and
    /// marks have no style.
    fn styled(&self, opts: Vec<GInfo>) -> Vec<GInfo> {
        let want: Vec<GInfo> = opts.iter().filter(|g| g.print == self.print || !crate::hand::has_letter(&g.text)).cloned().collect();
        if !want.is_empty() || (self.print && self.p.letters == Letters::Print) {
            want
        } else {
            opts
        }
    }

    pub fn pick(&mut self, key: &str, forced: Option<&String>) -> Option<GInfo> {
        let (k, options) = {
            let (k, o) = self.lookup(key)?;
            (k, o.clone())
        };
        let options = self.styled(options);
        if options.is_empty() {
            return None;
        }
        if let Some(f) = forced {
            if let Some(g) = options.iter().find(|g| &g.id == f) {
                return Some(g.clone());
            }
        }
        let keep = options.len().saturating_sub(1).min(3);
        // avoid repeating the last few variants of this letter
        let pool: Vec<GInfo> = {
            let recent = self.recent.get(&k);
            let fresh: Vec<GInfo> = options.iter().filter(|g| recent.is_none_or(|r| !r.contains(&g.id))).cloned().collect();
            if fresh.is_empty() { options.clone() } else { fresh }
        };
        // variants written in running text look more natural: weight them up
        let weights: Vec<f32> = pool.iter().map(|g| if g.src == Src::Text { 2.0 } else { 1.0 }).collect();
        let total: f32 = weights.iter().sum();
        let mut t = self.rng.random::<f32>() * total;
        let mut idx = pool.len() - 1;
        for (i, w) in weights.iter().enumerate() {
            if t < *w {
                idx = i;
                break;
            }
            t -= w;
        }
        let g = pool[idx].clone();
        let recent = self.recent.entry(k).or_default();
        recent.push_back(g.id.clone());
        while recent.len() > keep {
            recent.pop_front();
        }
        Some(g)
    }

    fn join_prob(&self, a: char, b: char) -> f32 {
        let [j, n] = self.pairs.get(&format!("{a}{b}")).copied().unwrap_or([0, 0]);
        let base = (j as f32 + 2.0 * self.join_rate) / (n as f32 + 2.0);
        (base * self.p.joins).min(1.0)
    }

    /// Rough written width (mm), without touching the random stream.
    pub fn estimate(&self, text: &str) -> f32 {
        let mut total = 0.0;
        for c in text.chars() {
            if crate::ingest::is_marker(c) {
                continue;
            }
            if c == ' ' {
                total += self.wgap[0] * self.xh;
                continue;
            }
            match self.lookup(&c.to_string()) {
                None => total += 0.6 * self.xh,
                Some((_, opts)) => {
                    let ws: Vec<f32> = opts.iter().take(4).map(|g| (g.ink_x[1] - g.ink_x[0]) / g.ppm * if g.src == Src::Box { self.box_scale } else { 1.0 }).collect();
                    total += crate::signal::median(&ws) * self.p.size + self.gap[0] * self.xh;
                }
            }
        }
        total
    }

    /// Versions for a whole word, chosen together (dynamic programming over the candidates):
    /// prefer versions written between the same neighbours, joining strokes that meet at the
    /// same height, and above all versions that stood next to each other in a written word;
    /// a little randomness and the recently used ones keep repeated words different.
    fn choose(&mut self, units: &[Unit]) -> Vec<Option<GInfo>> {
        const MAX_CANDIDATES: usize = 24;
        let mut cands: Vec<Vec<GInfo>> = vec![];
        let mut keys: Vec<String> = vec![];
        for u in units {
            let Some((k, opts)) = self.lookup(&u.key).map(|(k, o)| (k, o.clone())) else {
                cands.push(vec![]);
                keys.push(String::new());
                continue;
            };
            let mut opts = match u.forced.as_ref().and_then(|f| opts.iter().find(|g| &g.id == f)) {
                Some(g) => vec![g.clone()],
                None => self.styled(opts),
            };
            if opts.len() > MAX_CANDIDATES {
                // a random handful keeps the work small and the choice varied. First go the
                // versions written right after one of the letter before's (the join as it was
                // written), then those written between the same neighbours
                for i in (1..opts.len()).rev() {
                    let j = self.rng.random_range(0..=i);
                    opts.swap(i, j);
                }
                let before = cands.last().map(|c| c.as_slice()).unwrap_or(&[]);
                let rank = |g: &GInfo| {
                    if before.iter().any(|a| adjacent(a, g)) {
                        3
                    } else {
                        g.ctx.as_ref().map_or(0, |x| (x.prev == u.prev) as u8 + (x.next == u.next) as u8)
                    }
                };
                opts.sort_by_cached_key(|g| std::cmp::Reverse(rank(g)));
                opts.truncate(MAX_CANDIDATES);
            }
            cands.push(opts);
            keys.push(k);
        }
        let node = |me: &mut Self, i: usize, g: &GInfo| -> f32 {
            let u = &units[i];
            let mut c = me.rng.random::<f32>() * 0.6;
            if me.recent.get(&keys[i]).is_some_and(|r| r.contains(&g.id)) {
                c += 0.8;
            }
            c += match g.src {
                Src::Text => 0.0,
                Src::Box => 0.3,
                Src::Auto => 0.5,
            };
            if let Some(x) = &g.ctx {
                if x.prev == u.prev {
                    c -= 0.35;
                }
                if x.next == u.next {
                    c -= 0.35;
                }
                if me.recent_words.contains(&x.word) {
                    c += 0.5;
                }
            }
            c
        };
        let edge = |me: &Self, a: &GInfo, b: &GInfo| -> f32 {
            if adjacent(a, b) {
                return -1.2;
            }
            match (a.stub[1], b.stub[0], a.exit, b.entry) {
                (true, true, Some(ex), Some(en)) => {
                    let ya = (a.bl - ex[1]) / a.ppm;
                    let yb = (b.bl - en[1]) / b.ppm;
                    0.6 * ((ya - yb).abs() / me.xh.max(0.1)).min(2.0)
                }
                _ => 0.25,
            }
        };
        let n = units.len();
        let mut best: Vec<Vec<f32>> = vec![vec![]; n];
        let mut from: Vec<Vec<Option<usize>>> = vec![vec![]; n];
        for i in 0..n {
            let linked = i > 0 && !cands[i - 1].is_empty() && units[i - 1].mode == units[i].mode;
            for j in 0..cands[i].len() {
                let g = cands[i][j].clone();
                let own = node(self, i, &g);
                let (prev_cost, back) = if linked {
                    (0..cands[i - 1].len())
                        .map(|k| (best[i - 1][k] + edge(self, &cands[i - 1][k], &g), Some(k)))
                        .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap())
                        .unwrap()
                } else if i > 0 && !best[i - 1].is_empty() {
                    (best[i - 1].iter().cloned().fold(f32::INFINITY, f32::min), None)
                } else {
                    (0.0, None)
                };
                best[i].push(own + prev_cost);
                from[i].push(back);
            }
        }
        // read the choice back from the end; a unit without candidates restarts the chain
        let mut out: Vec<Option<GInfo>> = vec![None; n];
        let mut next: Option<usize> = None;
        for i in (0..n).rev() {
            if cands[i].is_empty() {
                next = None;
                continue;
            }
            let j = match next {
                Some(j) => j,
                None => (0..cands[i].len()).min_by(|&a, &b| best[i][a].partial_cmp(&best[i][b]).unwrap()).unwrap(),
            };
            out[i] = Some(cands[i][j].clone());
            next = from[i][j];
        }
        for (i, g) in out.iter().enumerate() {
            let Some(g) = g else { continue };
            let keep = cands[i].len().saturating_sub(1).min(3);
            let recent = self.recent.entry(keys[i].clone()).or_default();
            recent.push_back(g.id.clone());
            while recent.len() > keep {
                recent.pop_front();
            }
        }
        for g in out.iter().flatten() {
            if let Some(x) = &g.ctx {
                if !self.recent_words.contains(&x.word) {
                    self.recent_words.push_back(x.word);
                }
            }
        }
        while self.recent_words.len() > 8 {
            self.recent_words.pop_front();
        }
        out
    }

    /// Compose one word from x = 0. Returns elements (l unset = NaN) and advance width (mm).
    pub fn word(&mut self, text: &str, wi: u32) -> (Vec<Elem>, f32) {
        let chars: Vec<char> = text.chars().collect();
        // 1. the plan: formatting marks and the glyph units (single letters or ligatures)
        let mut plan: Vec<Result<Unit, char>> = vec![];
        let mut mode: Option<char> = None;
        let (mut i, mut ci) = (0usize, 0u32);
        while i < chars.len() {
            let ch = chars[i];
            if crate::ingest::is_marker(ch) {
                if ch == SUP || ch == SUB {
                    mode = Some(ch);
                } else if ch == END {
                    mode = None;
                }
                plan.push(Err(ch));
                i += 1;
                continue;
            }
            let mut key = ch.to_string();
            if mode.is_none() && charset::is_letter(ch) {
                for lig in self.ligs.clone() {
                    // printed letters stand apart: no letters written together, but oʻ is one
                    if self.print && !lig.contains('ʻ') {
                        continue;
                    }
                    let lc: Vec<char> = lig.chars().collect();
                    // a mark written as part of its letter (oʻ, gʻ) always goes with it
                    let p = if lig.contains('ʻ') { 1.0 } else { 0.65 };
                    if chars[i..].starts_with(&lc) && self.rng.random::<f32>() < p {
                        key = lig;
                        break;
                    }
                }
            }
            let klen = key.chars().count();
            let forced = self.p.overrides.get(&format!("{wi}:{ci}")).cloned();
            plan.push(Ok(Unit { key, ci, mode, prev: None, next: None, forced }));
            i += klen;
            ci += klen as u32;
        }
        let idx: Vec<usize> = (0..plan.len()).filter(|&k| plan[k].is_ok()).collect();
        let mut units: Vec<Unit> = idx.iter().map(|&k| plan[k].clone().unwrap()).collect();
        for k in 0..units.len() {
            units[k].prev = k.checked_sub(1).and_then(|p| units[p].key.chars().last());
            units[k].next = units.get(k + 1).and_then(|u| u.key.chars().next());
        }
        // 2. the versions, chosen for the whole word
        let chosen = self.choose(&units);
        // 3. placing them
        let mut out = vec![];
        let mut x = 0.0f32;
        let mut prev: Option<(GInfo, [f32; 6], char, Option<char>)> = None; // glyph, (x,dy,s,r,sh,bl), last char, mode
        let slant = (self.p.slant + 4.0 * (self.p.speed - 0.3)).to_radians().tan();
        let mut ul_start: Option<f32> = self.under.then_some(0.0);
        let mut u = 0;
        for step in &plan {
            let unit = match step {
                Err(ch) => {
                    match *ch {
                        BOLD | BOLD_END => self.bold = *ch == BOLD,
                        UNDER => {
                            self.under = true;
                            ul_start = Some(if out.is_empty() { 0.0 } else { x + 0.1 * self.xh });
                        }
                        UNDER_END => {
                            self.under = false;
                            if let Some(s0) = ul_start.take() {
                                out.push(self.underline(s0, x));
                            }
                        }
                        _ => {}
                    }
                    continue;
                }
                Ok(unit) => unit,
            };
            let g = chosen[u].clone();
            u += 1;
            let key = &unit.key;
            let ch = key.chars().next().unwrap();
            let Some(g) = g else {
                // written, but not in print
                if self.print && self.p.letters == Letters::Print && self.has(key) {
                    self.missing_print.insert(ch);
                } else {
                    self.missing.insert(ch);
                }
                x += 0.6 * self.xh;
                prev = None;
                continue;
            };
            let mode = unit.mode;
            let seed = self.rng.random::<u32>();
            let p = self.pressure * if self.bold { 1.25 } else { 1.0 };
            let last = key.chars().last().unwrap();
            // written next to the previous version in a real word: put it back exactly there
            if let Some((pg, pe, _, pm)) = &prev {
                if *pm == mode && adjacent(pg, &g) {
                    let (a, b) = (pg.ctx.as_ref().unwrap(), g.ctx.as_ref().unwrap());
                    let (gx, gdy) = glyph_point(pe[0], pe[1], pe[2], pe[3], pe[4], pe[5], b.x - a.x, pg.bl + (b.y - a.y));
                    let (s, r, sh) = (pe[2], pe[3], pe[4]);
                    out.push(Elem::G { gid: g.id.clone(), x: gx, l: f32::NAN, dy: gdy, s, r, sh, seed, wi, ci: unit.ci, p });
                    x = x.max(gx + g.ink_x[1] * s);
                    prev = Some((g.clone(), [gx, gdy, s, r, sh, g.bl], last, mode));
                    continue;
                }
            }
            let small = if mode.is_some() { 0.62 } else { 1.0 };
            let mut s = (1.0 / g.ppm) * self.p.size * self.fatigue * self.size_mul * small;
            s *= if g.src == Src::Box { self.box_scale } else { 1.0 } * (1.0 + self.n(0.035));
            let base_dy = self.n(0.035 * self.xh)
                + match mode {
                    Some(SUP) => -0.75 * self.xh,
                    Some(_) => 0.35 * self.xh,
                    None => 0.0,
                };
            let r = self.n(1.6);
            let sh = self.n(0.025) + slant;
            let first = ch;
            let mut gx = 0.0;
            let mut joined = false;
            if let (Some((pg, pe, pc, _)), Some(en)) = (&prev, g.entry) {
                if let Some(ex) = pg.exit {
                    if mode.is_none() && !self.print && charset::is_letter(first) && charset::is_letter(*pc) && self.rng.random::<f32>() < self.join_prob(*pc, first) {
                        let e = glyph_point(pe[0], pe[1], pe[2], pe[3], pe[4], pe[5], ex[0], ex[1]);
                        let n0 = glyph_point(0.0, base_dy, s, r, sh, g.bl, en[0], en[1]);
                        // both ends are halves of real joining strokes: they nearly meet
                        let stubs = pg.stub[1] && g.stub[0];
                        // letters stay on the baseline; the join bridges different heights
                        if (e.1 - n0.1).abs() < if stubs { 0.8 } else { 0.6 } * self.xh {
                            let len = if stubs { (0.06 + self.n(0.03)).max(0.0) } else { (0.32 + self.n(0.07)).max(0.15) } * self.xh;
                            gx = e.0 + len - n0.0;
                            let n1 = glyph_point(gx, base_dy, s, r, sh, g.bl, en[0], en[1]);
                            out.push(connector(e, [ex[2], ex[3]], n1, [en[2], en[3]], 0.9 * self.pen_mm * self.p.size, &mut self.rng, self.pressure));
                            joined = true;
                        }
                    }
                }
            }
            if !joined {
                let gap = if prev.is_none() { 0.0 } else { (self.gap[0] + self.n(self.gap[1])).max(0.05) * self.xh * self.fatigue };
                gx = x + gap - g.ink_x[0] * s;
            }
            out.push(Elem::G { gid: g.id.clone(), x: gx, l: f32::NAN, dy: base_dy, s, r, sh, seed, wi, ci: unit.ci, p });
            x = x.max(gx + g.ink_x[1] * s);
            prev = Some((g.clone(), [gx, base_dy, s, r, sh, g.bl], last, mode));
        }
        if let Some(s0) = ul_start {
            out.push(self.underline(s0, x));
        }
        (out, x)
    }

    /// A quick pen line under part of a word.
    fn underline(&mut self, x0: f32, x1: f32) -> Elem {
        let y = 0.32 * self.xh + self.n(0.04 * self.xh);
        let tilt = self.n(0.05 * self.xh);
        let n = 10;
        let pts: Vec<[f32; 3]> = (0..n)
            .map(|k| {
                let t = k as f32 / (n - 1) as f32;
                [x0 - 0.3 + t * (x1 - x0 + 0.6), f32::NAN, y + tilt * (t - 0.5) + 0.03 * self.xh * (t * 7.0).sin()]
            })
            .collect();
        Elem::S { w: vec![0.85 * self.pen_mm * self.p.size; n], pts, p: self.pressure }
    }
}

/// Cubic Bezier from an exit to the next entry, leaving and arriving along the strokes.
/// Directions are kept forward (positive x) so the curve never loops.
pub fn connector(p0: (f32, f32), d0: [f32; 2], p1: (f32, f32), d1: [f32; 2], width: f32, rng: &mut Pcg64Mcg, p: f32) -> Elem {
    let fwd = |d: [f32; 2]| {
        let n = (d[0] * d[0] + d[1] * d[1]).sqrt().max(1e-9);
        let mut v = [d[0] / n, d[1] / n];
        if v[0] < 0.35 {
            v[0] = 0.35;
        }
        let m = (v[0] * v[0] + v[1] * v[1]).sqrt();
        [v[0] / m, v[1] / m]
    };
    let len = ((p1.0 - p0.0).powi(2) + (p1.1 - p0.1).powi(2)).sqrt();
    let (a, b) = (fwd(d0), fwd(d1));
    let c0 = (p0.0 + a[0] * len * 0.4, p0.1 + a[1] * len * 0.4);
    let c1 = (p1.0 - b[0] * len * 0.4, p1.1 - b[1] * len * 0.4);
    let mut pts = vec![];
    let mut w = vec![];
    for k in 0..12 {
        let t = k as f32 / 11.0;
        let u = 1.0 - t;
        let x = u * u * u * p0.0 + 3.0 * u * u * t * c0.0 + 3.0 * u * t * t * c1.0 + t * t * t * p1.0;
        let y = u * u * u * p0.1 + 3.0 * u * u * t * c0.1 + 3.0 * u * t * t * c1.1 + t * t * t * p1.1;
        pts.push([x, f32::NAN, y]);
        let z: f32 = rng.sample(StandardNormal);
        w.push(width * (1.0 - 0.25 * (std::f32::consts::PI * t).sin()) * (1.0 + 0.03 * z));
    }
    Elem::S { pts, w, p }
}

/// Places words onto lines and pages, adding per-line human drift.
pub struct Flow<'a> {
    pub c: &'a mut Composer,
    /// the geometry of the page being written
    pub g: Geometry,
    /// page i is written on geos[i % len] (a notebook: left and right pages in turn)
    geos: Vec<Geometry>,
    rng: Pcg64Mcg,
    pub pages: Vec<Vec<Elem>>,
    pub l: usize,
    pub wi: u32,
    l_dy: f32,
    l_skew: f32,
    l_ph: [f32; 2],
    l_x0: f32,
    words_done: u32,
    /// the space left after the word before (narrowed around a sign such as = or +)
    prev_gap: f32,
}

impl<'a> Flow<'a> {
    pub fn new(c: &'a mut Composer, geos: Vec<Geometry>) -> Self {
        // separate random stream: letter choices stay the same whatever the paper
        let rng = Pcg64Mcg::seed_from_u64(c.p.seed.wrapping_add(7919));
        let geos = if geos.is_empty() { vec![Geometry::default()] } else { geos };
        let g = geos[0].clone();
        let mut f = Flow { c, g, geos, rng, pages: vec![vec![]], l: 0, wi: 0, l_dy: 0.0, l_skew: 0.0, l_ph: [0.0; 2], l_x0: 0.0, words_done: 0, prev_gap: 0.0 };
        f.line_noise();
        f
    }

    pub fn n(&mut self, sd: f32) -> f32 {
        let z: f32 = self.rng.sample(StandardNormal);
        z * sd * self.c.p.mess
    }

    fn line_noise(&mut self) {
        let xh = self.c.xh;
        let drift = self.c.p.line_drift * (1.0 + 0.8 * (self.c.p.speed - 0.3)).max(0.2);
        let ruled = self.g.ruled;
        self.l_dy = -0.04 * xh + self.n((if ruled { 0.04 } else { 0.1 }) * xh * drift);
        self.l_skew = self.n((if ruled { 0.15 } else { 0.6 }) * drift);
        self.l_ph = [self.rng.random::<f32>() * 6.283, self.rng.random::<f32>() * 6.283];
        self.l_x0 = self.n(0.8);
        let prog = self.l as f32 / (self.g.lines_per_page.max(2) - 1) as f32;
        self.c.fatigue = 1.0 + 0.03 * prog * self.c.p.mess;
    }

    fn drift(&self, x: f32) -> f32 {
        let a = (if self.g.ruled { 0.035 } else { 0.06 }) * self.c.xh * self.c.p.mess * self.c.p.line_drift;
        let wob = a * ((x / 9.0 + self.l_ph[0]).sin() + 0.5 * (x / 3.7 + self.l_ph[1]).sin());
        self.l_dy + self.l_skew.to_radians().tan() * x + wob
    }

    pub fn newline(&mut self) {
        self.l += 1;
        if self.l >= self.g.lines_per_page {
            self.newpage();
            return;
        }
        self.line_noise();
    }

    pub fn newpage(&mut self) {
        self.pages.push(vec![]);
        self.g = self.geos[(self.pages.len() - 1) % self.geos.len()].clone();
        self.l = 0;
        self.line_noise();
    }

    /// Width of the line being written.
    pub fn width(&self) -> f32 {
        self.g.widths.get(self.l).copied().unwrap_or(self.g.width_mm)
    }

    pub fn room(&self, lines: usize) -> bool {
        self.l + lines <= self.g.lines_per_page
    }

    pub fn put(&mut self, elems: Vec<Elem>, x0: f32, l: Option<f32>, follow: bool) {
        let l = l.unwrap_or(self.l as f32);
        let mut placed = vec![];
        for e in elems {
            match e {
                Elem::G { gid, x, dy, s, r, sh, seed, wi, ci, p, .. } => {
                    let x = x + x0;
                    let (dy, r) = if follow { (dy + self.drift(x), r + self.l_skew) } else { (dy, r) };
                    placed.push(Elem::G { gid, x, l, dy, s, r, sh, seed, wi, ci, p });
                }
                Elem::S { pts, w, p } => {
                    let pts = pts
                        .into_iter()
                        .map(|[x, pl, dy]| {
                            let x = x + x0;
                            [x, if pl.is_nan() { l } else { pl }, dy + if follow { self.drift(x) } else { 0.0 }]
                        })
                        .collect();
                    placed.push(Elem::S { pts, w, p });
                }
            }
        }
        self.pages.last_mut().unwrap().extend(placed);
    }

    /// Flow text across lines. `first_x` / `next_x`: start of the first / following lines.
    /// Every space counts: more than one between words leaves more room (a letter's width
    /// each, as on a keyboard), some at the start indent; a line break inside the text starts a
    /// new line.
    pub fn text(&mut self, text: &str, first_x: f32, next_x: f32, center: bool) {
        let mut x = first_x + self.l_x0;
        let mut line: Vec<(Vec<Elem>, f32, f32)> = vec![];
        let mut spaces = 0;
        for (k, part) in text.split(crate::ingest::LINE_BREAK).enumerate() {
            if k > 0 {
                self.flush(&mut line, center);
                self.newline();
                x = next_x + self.l_x0;
                spaces = 0;
            }
            for w in part.split(' ') {
                if w.is_empty() {
                    spaces += 1;
                    continue;
                }
                x += spaces as f32 * self.space();
                spaces = 0;
                self.word(w, &mut x, &mut line, next_x, center);
            }
        }
        self.flush(&mut line, center);
    }

    /// One space more: about a letter's width.
    fn space(&self) -> f32 {
        0.9 * self.c.xh * self.c.fatigue
    }

    /// One word at `x` on the line being written (or the next, when it does not fit).
    fn word(&mut self, w: &str, x: &mut f32, line: &mut Vec<(Vec<Elem>, f32, f32)>, next_x: f32, center: bool) {
        {
            // = + : ≈ … written as words of their own: people leave less room around them
            let sign = is_sign(w);
            if sign && !line.is_empty() {
                *x -= 0.5 * self.prev_gap;
            }
            let (mut el, mut ww) = self.compose_word(w);
            if !line.is_empty() && *x + ww > self.width() {
                let over = *x + ww - self.width();
                if over < 0.04 * self.width() {
                    // squeeze the last word in, like people do
                    el = el.into_iter().map(|e| squeeze(e, 0.93)).collect();
                    ww *= 0.93;
                } else {
                    self.flush(line, center);
                    self.newline();
                    *x = next_x + self.l_x0;
                }
            }
            line.push((el, *x, ww));
            // never so narrow that two words read as one
            let gap = (self.c.wgap[0] + self.n(self.c.wgap[1])).max(0.6 * self.c.wgap[0]).max(0.4) * self.c.xh * self.c.fatigue;
            let gap = if sign { 0.5 * gap } else { gap };
            self.prev_gap = gap;
            *x += ww + gap;
        }
    }

    fn flush(&mut self, line: &mut Vec<(Vec<Elem>, f32, f32)>, center: bool) {
        if line.is_empty() {
            return;
        }
        let shift = if center {
            let total = line.last().map(|l| l.1 + l.2).unwrap() - line[0].1;
            ((self.width() - total) / 2.0 - line[0].1).max(0.0)
        } else {
            0.0
        };
        for (el, wx, _) in line.drain(..) {
            self.put(el, wx + shift, None, true);
        }
    }

    /// A word, sometimes preceded by a crossed-out misspelling (realism option).
    fn compose_word(&mut self, w: &str) -> (Vec<Elem>, f32) {
        let wi = self.wi;
        self.wi += 1;
        self.words_done += 1;
        let p_mistake = self.c.p.mistakes / 100.0;
        if p_mistake > 0.0 && w.chars().count() >= 4 && self.rng.random::<f32>() < p_mistake {
            let wrong = misspell(w, &mut self.rng);
            let (mut bad, bw) = self.c.word(&wrong, u32::MAX - wi);
            bad.extend(strike(bw, self.c.xh, self.c.pen_mm, &mut self.rng, self.c.pressure));
            let (good, gw) = self.c.word(w, wi);
            let gap = self.c.wgap[0] * self.c.xh;
            let mut all = bad;
            all.extend(good.into_iter().map(|e| shift_x(e, bw + gap)));
            return (all, bw + gap + gw);
        }
        self.c.word(w, wi)
    }

    pub fn hline(&mut self, x0: f32, x1: f32, l: f32, dy: f32, ruler: bool) {
        let e = drawn_line([x0, f32::NAN, dy], [x1, f32::NAN, dy], self, ruler);
        self.put(vec![e], 0.0, Some(l), !ruler);
    }

    pub fn vline(&mut self, x: f32, l0: f32, l1: f32, ruler: bool) {
        let e = drawn_line([x, l0, 0.0], [x, l1, 0.0], self, ruler);
        self.put(vec![e], 0.0, Some(l0), false);
    }
}

/// A word that is only a sign of a formula (not a dash, which also stands between words).
fn is_sign(w: &str) -> bool {
    w.chars().count() <= 2 && w.chars().all(|c| "=+:·×÷≈≤≥±→<>≠".contains(c))
}

fn shift_x(e: Elem, dx: f32) -> Elem {
    match e {
        Elem::G { gid, x, l, dy, s, r, sh, seed, wi, ci, p } => Elem::G { gid, x: x + dx, l, dy, s, r, sh, seed, wi, ci, p },
        Elem::S { pts, w, p } => Elem::S { pts: pts.into_iter().map(|[x, l, d]| [x + dx, l, d]).collect(), w, p },
    }
}

fn squeeze(e: Elem, f: f32) -> Elem {
    match e {
        Elem::G { gid, x, l, dy, s, r, sh, seed, wi, ci, p } => Elem::G { gid, x: x * f, l, dy, s: s * 0.97, r, sh, seed, wi, ci, p },
        Elem::S { pts, w, p } => Elem::S { pts: pts.into_iter().map(|[x, l, d]| [x * f, l, d]).collect(), w, p },
    }
}

/// Plausible slip: swap two neighbouring letters, double one, or drop one.
fn misspell(w: &str, rng: &mut Pcg64Mcg) -> String {
    let mut c: Vec<char> = w.chars().collect();
    let n = c.len();
    let i = rng.random_range(1..n - 1);
    match rng.random_range(0..3) {
        0 => c.swap(i, i + 1),
        1 => c.insert(i, c[i]),
        _ => {
            c.remove(i);
        }
    }
    let cut = rng.random_range(2..c.len().max(3)).min(c.len());
    c.truncate(cut.max(2));
    c.into_iter().collect()
}

/// One or two quick strike-through lines over a written word.
fn strike(width: f32, xh: f32, pen: f32, rng: &mut Pcg64Mcg, p: f32) -> Vec<Elem> {
    let mut out = vec![];
    let passes = if rng.random::<f32>() < 0.4 { 2 } else { 1 };
    for k in 0..passes {
        let y = -0.45 * xh + (k as f32 * 0.25 - 0.1) * xh;
        let tilt = (rng.random::<f32>() - 0.5) * 0.25 * xh;
        let pts: Vec<[f32; 3]> = (0..10).map(|i| {
            let t = i as f32 / 9.0;
            [-0.6 + t * (width + 1.2), f32::NAN, y + tilt * (t - 0.5) + 0.05 * xh * (t * 9.0).sin()]
        }).collect();
        out.push(Elem::S { w: vec![pen * 0.9; pts.len()], pts, p });
    }
    out
}

/// A pen line from a to b ([x, l, dy]); slight wobble, overshoot and width variation.
fn drawn_line(a: [f32; 3], b: [f32; 3], f: &mut Flow, ruler: bool) -> Elem {
    let horizontal = a[1].is_nan() || a[1] == b[1];
    // a point every 2.5 mm or so: the line follows the paper (its lines bend) smoothly
    let len = if horizontal { (b[0] - a[0]).abs() } else { (b[1] - a[1]).abs() * f.g.pitch_mm };
    let n = ((len / 2.5).ceil() as usize).clamp(24, 160);
    let (o0, o1) = (f.n(0.8), f.n(0.8));
    let amp = (if ruler { 0.08 } else { 0.35 }) * f.c.p.mess;
    let ph = f.rng.random::<f32>() * 6.283;
    let freq = 0.6 + f.rng.random::<f32>();
    let mut pts = vec![];
    let mut w = vec![];
    for i in 0..n {
        let t = i as f32 / (n - 1) as f32;
        let z: f32 = f.rng.sample(StandardNormal);
        let wv = amp * (std::f32::consts::PI * t * freq + ph).sin() + 0.02 * z;
        if horizontal {
            pts.push([a[0] - o0 + (b[0] + o1 - a[0] + o0) * t, a[1], a[2] + wv]);
        } else {
            pts.push([a[0] + wv, a[1] + (b[1] - a[1]) * t, a[2]]);
        }
        let z2: f32 = f.rng.sample(StandardNormal);
        let mut wi = 0.85 * f.c.pen_mm * (1.0 + 0.12 * (t * 3.0 + ph).sin() + 0.03 * z2);
        if i < 2 {
            wi *= 0.85;
        }
        if i >= n - 2 {
            wi *= 0.8;
        }
        w.push(wi);
    }
    Elem::S { pts, w, p: f.c.pressure }
}

pub fn layout(blocks: &[Block], hand: &Hand, p: &Params, geo: Geometry) -> LayoutResult {
    layout_pages(blocks, hand, p, vec![geo])
}

/// Lay out onto pages of different papers: page i on geos[i % len] (a notebook's left and
/// right pages, each with its own margins and lines).
pub fn layout_pages(blocks: &[Block], hand: &Hand, p: &Params, geos: Vec<Geometry>) -> LayoutResult {
    let mut c = Composer::new(hand, p.clone());
    let xh;
    let pen;
    let pages;
    {
        let mut f = Flow::new(&mut c, geos);
        let mut first = true;
        for b in blocks {
            // print letters for this block, or the person's joined writing
            f.c.print = match p.letters {
                Letters::Joined => false,
                Letters::Print => true,
                Letters::Auto => charset::is_english(&block_text(b)),
            };
            if matches!(b, Block::Br) {
                f.newpage();
                first = true;
                continue;
            }
            if !first && !matches!(b, Block::Table { .. }) {
                f.newline();
            }
            first = false;
            match b {
                Block::P { text } => f.text(text, p.indent_mm, 0.0, false),
                Block::H { text, level } => {
                    let big = *level <= 2;
                    f.c.size_mul = if big { 1.1 } else { 1.0 };
                    f.c.pressure = 1.15;
                    f.text(text, 0.0, 0.0, big);
                    f.c.size_mul = 1.0;
                    f.c.pressure = 1.0;
                }
                Block::Li { text, marker } => f.text(&format!("{marker} {text}"), 3.0, 9.0, false),
                Block::Rule => {
                    let (l, pitch, w) = (f.l as f32, f.g.pitch_mm, f.width());
                    f.hline(0.0, w, l, -0.35 * pitch, true);
                }
                Block::Math { tex } => {
                    // 2-D formula, centred; tall ones take the ruled lines they need
                    let m = crate::math::parse(tex);
                    let mut wi = f.wi;
                    let b = crate::math::layout(&m, f.c, 1.0, &mut wi);
                    f.wi = wi;
                    if b.elems.is_empty() {
                        f.text(&latex_to_text(tex), 0.0, 0.0, true);
                    } else {
                        let pitch = f.g.pitch_mm;
                        let above = ((b.asc - 0.85 * pitch) / pitch).ceil().max(0.0) as usize;
                        let below = ((b.desc - 0.3 * pitch) / pitch).ceil().max(0.0) as usize;
                        for _ in 0..above {
                            f.newline();
                        }
                        let x0 = ((f.width() - b.w) / 2.0).max(0.0);
                        f.put(b.elems, x0, None, true);
                        for _ in 0..below {
                            f.newline();
                        }
                    }
                }
                Block::Table { rows } => table(rows, &mut f),
                Block::Br => {}
            }
        }
        pages = std::mem::take(&mut f.pages);
        xh = f.c.xh;
        pen = f.c.pen_mm;
    }
    LayoutResult { pages, missing: c.missing.into_iter().collect(), missing_print: c.missing_print.into_iter().collect(), xh, pen_mm: pen }
}

/// The words of a block, to tell its language.
fn block_text(b: &Block) -> String {
    match b {
        Block::P { text } | Block::H { text, .. } | Block::Li { text, .. } => text.clone(),
        Block::Table { rows } => rows.iter().flatten().cloned().collect::<Vec<_>>().join(" "),
        _ => String::new(),
    }
}

fn table(rows: &[Vec<String>], f: &mut Flow) {
    if rows.is_empty() {
        return;
    }
    let ncol = rows.iter().map(|r| r.len()).max().unwrap_or(1).max(1);
    let rows: Vec<Vec<String>> = rows.iter().map(|r| {
        let mut r = r.clone();
        r.resize(ncol, String::new());
        r
    }).collect();
    let pad = 2.0;
    let gap = f.c.wgap[0] * f.c.xh;
    // every word written first: a column is as wide as the writing in it really is (an
    // estimate let numbers run over the borders)
    type Word = (Vec<Elem>, f32);
    let words: Vec<Vec<Vec<Word>>> = rows
        .iter()
        .map(|r| {
            r.iter()
                .map(|t| {
                    t.split_whitespace()
                        .map(|w| {
                            let wi = f.wi;
                            f.wi += 1;
                            f.c.word(w, wi)
                        })
                        .collect()
                })
                .collect()
        })
        .collect();
    let span = |ws: &[Word]| ws.iter().map(|w| w.1).sum::<f32>() + gap * ws.len().saturating_sub(1) as f32;
    let nat: Vec<f32> = (0..ncol).map(|j| (words.iter().map(|r| span(&r[j])).fold(0.0, f32::max) + 2.0 * pad).max(6.0)).collect();
    let scale = (f.width() / nat.iter().sum::<f32>()).min(1.0);
    let widths: Vec<f32> = nat.iter().map(|w| w * scale).collect();
    let mut xs = vec![0.0f32];
    for w in &widths {
        xs.push(xs.last().unwrap() + w);
    }
    // a cell's words in lines that fit its column
    let split = |ws: Vec<Word>, width: f32| -> Vec<Vec<Word>> {
        let mut lines = vec![];
        let (mut cur, mut x): (Vec<Word>, f32) = (vec![], 0.0);
        for w in ws {
            if !cur.is_empty() && x + gap + w.1 > width - 2.0 * pad {
                lines.push(std::mem::take(&mut cur));
                x = 0.0;
            }
            x += if cur.is_empty() { w.1 } else { gap + w.1 };
            cur.push(w);
        }
        if !cur.is_empty() || lines.is_empty() {
            lines.push(cur);
        }
        lines
    };
    let cells: Vec<Vec<Vec<Vec<Word>>>> = words.into_iter().map(|r| r.into_iter().enumerate().map(|(j, ws)| split(ws, widths[j])).collect()).collect();
    // borders sit on the paper's lines; each row's text sits in the middle of its row
    let lift = ((f.g.pitch_mm - 1.6 * f.c.xh) / 2.0).max(0.18 * f.g.pitch_mm);
    f.newline();
    let mut top = f.l;
    let (x0, x1) = (xs[0], *xs.last().unwrap());
    f.hline(x0, x1, top as f32, 0.0, true);
    for row in cells {
        let h = row.iter().map(|c| c.len()).max().unwrap_or(1);
        if !f.room(h + 1) {
            verticals(f, &xs, top, f.l);
            f.newpage();
            top = f.l;
            f.hline(x0, x1, top as f32, 0.0, true);
        }
        let mut row: Vec<std::vec::IntoIter<Vec<Word>>> = row.into_iter().map(|c| c.into_iter()).collect();
        for _ in 0..h {
            f.newline();
            for (j, cl) in row.iter_mut().enumerate() {
                if let Some(line) = cl.next() {
                    let mut x = xs[j] + pad;
                    for (el, ww) in line {
                        let el = el.into_iter().map(|e| match e {
                            Elem::G { gid, x, l, dy, s, r, sh, seed, wi, ci, p } => Elem::G { gid, x, l, dy: dy - lift, s, r, sh, seed, wi, ci, p },
                            Elem::S { pts, w, p } => Elem::S { pts: pts.into_iter().map(|[a, b, d]| [a, b, d - lift]).collect(), w, p },
                        }).collect();
                        f.put(el, x, None, true);
                        x += ww + gap;
                    }
                }
            }
        }
        let l = f.l as f32;
        f.hline(x0, x1, l, 0.0, true);
    }
    let l = f.l;
    verticals(f, &xs, top, l);
}

fn verticals(f: &mut Flow, xs: &[f32], top: usize, bottom: usize) {
    if bottom <= top {
        return;
    }
    for &x in xs {
        f.vline(x, top as f32, bottom as f32, true);
    }
}
