//! Cut a handwritten line into letters using its known transcript.
//!
//! Words are found from the widest gaps. Inside a word, joined letters are cut where a single
//! thin stroke crosses a column near the baseline (the connecting stroke between letters): this
//! over-cuts on purpose (u, w split in two), and dynamic programming then groups the pieces
//! into the transcript's characters, scoring widths and which pieces reach above the x-height
//! (b d h k l …) or below the baseline (g j p q y …). One piece can still hold several letters
//! (kept as a ligature glyph), and several pieces can form one letter.

use std::collections::{HashMap, HashSet};

use crate::charset;
use crate::img::{self, Gray};
use crate::signal;

/// This person's letter widths (in units of their average letter), learned from a first pass
/// over their pages. Characters not seen often enough keep the general guess.
#[derive(Clone, Debug, Default)]
pub struct Widths {
    seen: HashMap<char, Vec<f64>>,
    learned: HashMap<char, f64>,
    /// typical shapes of the person's letters (`hand::shape`), from pages already cut: the
    /// cutter prefers groupings that look like them
    pub shapes: HashMap<char, Vec<Vec<f32>>>,
}

/// How much a grouping looks like the person's typical versions of `c` (None: too few known).
fn shape_match(widths: &Widths, c: char, shape: &[f32]) -> Option<f32> {
    let protos = widths.shapes.get(&c).filter(|p| p.len() >= 3)?;
    let mut sims: Vec<f32> = protos.iter().map(|p| crate::hand::shape_sim(shape, p)).collect();
    sims.sort_by(|a, b| b.partial_cmp(a).unwrap());
    // the best two: one lucky look-alike is not enough
    Some((sims[0] + sims[1]) / 2.0)
}

/// The shape of pieces k0..k1 of a word, as `hand::shape` measures a letter.
fn seg_shape(alpha: &Gray, wd: &Word, k0: usize, k1: usize, bl: &dyn Fn(f32) -> f32, xh: f32) -> Vec<f32> {
    let pieces = &wd.prims[k0..k1];
    let labels: HashSet<u32> = pieces.iter().flat_map(|g| g.labels.iter().copied()).collect();
    let x0 = pieces.iter().map(|g| g.x0).min().unwrap_or(0);
    let x1 = pieces.iter().map(|g| g.x1).max().unwrap_or(x0 + 1).max(x0 + 1);
    let w = x1 - x0;
    let mut g = Gray::new(w, wd.h, 0.0);
    for y in 0..wd.h {
        for x in x0..x1 {
            if labels.contains(&wd.lab[y * wd.w + x]) {
                g.d[y * w + x - x0] = alpha.at(wd.ox + x, wd.oy + y);
            }
        }
    }
    let cx = wd.ox as f32 + (x0 + x1) as f32 / 2.0;
    crate::hand::shape(&g, bl(cx) - wd.oy as f32, xh)
}

impl Widths {
    pub fn weight(&self, c: char) -> f64 {
        self.learned.get(&c).copied().unwrap_or_else(|| char_weight(c))
    }

    /// Blend what was measured with the general guess (3 samples' worth of prior).
    pub fn finish(&mut self) {
        for (c, v) in &self.seen {
            if v.len() >= 3 {
                let m = signal::median(&v.iter().map(|&x| x as f32).collect::<Vec<_>>()) as f64;
                let n = v.len() as f64;
                self.learned.insert(*c, (n * m + 3.0 * char_weight(*c)) / (n + 3.0));
            }
        }
        // widths are relative: keep the average letter at 1
        let all: Vec<f32> = self.learned.values().map(|&v| v as f32).collect();
        if all.len() >= 5 {
            let mean = signal::mean(&all) as f64;
            let prior: Vec<f32> = self.learned.keys().map(|&c| char_weight(c) as f32).collect();
            let k = signal::mean(&prior) as f64 / mean.max(1e-6);
            self.learned.values_mut().for_each(|v| *v *= k);
        }
    }
}

/// Relative written width of a character (general guess).
fn char_weight(c: char) -> f64 {
    if c == '#' {
        // stands for a whole crossed-out word
        4.0
    } else if "il1.,:;!'ʻʼ’|()[]ıj".contains(c) {
        0.5
    } else if "tfrecsz".contains(c) {
        0.85
    } else if "mwшщжюыМWШЩЖЮЫ".contains(c) {
        1.55
    } else if c.is_uppercase() {
        1.35
    } else {
        1.0
    }
}

/// Reaches above the x-height: Some(true) / Some(false), None = either (dots, marks, t).
fn expects_ascender(c: char) -> Option<bool> {
    if "ijtʻʼ'’ёйўЁЙЎ".contains(c) || !c.is_alphanumeric() {
        None
    } else if "bdfhklбвф".contains(c) || c.is_uppercase() || c.is_ascii_digit() {
        Some(true)
    } else {
        Some(false)
    }
}

/// Hangs below the baseline.
fn expects_descender(c: char) -> Option<bool> {
    if "zʻʼ'’зЗ".contains(c) || !c.is_alphanumeric() {
        None
    } else if "fgjpqyдруфцщ".contains(c) {
        Some(true)
    } else if c.is_uppercase() || c.is_ascii_digit() {
        None
    } else {
        Some(false)
    }
}

/// The mark of oʻ / gʻ: part of the letter. (The tutuq belgisi ʼ is a sign of its own.)
fn is_mark(c: char) -> bool {
    c == 'ʻ'
}

#[derive(Clone, Debug)]
struct Group {
    labels: Vec<u32>,
    x0: usize,
    x1: usize,
    y0: usize,
    y1: usize,
}

/// Ink pieces that are not writing, standing apart from it: faint smudges, and thin upright
/// bits of a printed line (a grid line left over near the margin). They would count as words.
fn drop_stray(gs: Vec<Group>, lab: &[u32], alpha: &Gray, ppm: f32) -> Vec<Group> {
    if gs.len() < 2 {
        return gs;
    }
    let n = lab.iter().copied().max().unwrap_or(0) as usize + 1;
    let (mut sum, mut cnt) = (vec![0.0f32; n], vec![0usize; n]);
    for (i, &l) in lab.iter().enumerate() {
        if l != 0 {
            sum[l as usize] += alpha.d[i];
            cnt[l as usize] += 1;
        }
    }
    let apart = 1.5 * ppm as f64;
    let keep: Vec<bool> = (0..gs.len())
        .map(|k| {
            let g = &gs[k];
            let (s, c) = g.labels.iter().fold((0.0, 0), |a, &l| (a.0 + sum[l as usize], a.1 + cnt[l as usize]));
            let faint = c > 0 && s / (c as f32) < 0.45;
            let (w, h) = ((g.x1 - g.x0) as f64, (g.y1 - g.y0) as f64);
            let thin = w <= 0.35 * ppm as f64 && h >= 3.0 * w.max(1.0);
            // room on both sides (or the row's end)
            let left = gs.iter().enumerate().filter(|(j, o)| *j != k && o.x1 <= g.x0 + 1).map(|(_, o)| g.x0 as f64 - o.x1 as f64).fold(f64::MAX, f64::min);
            let right = gs.iter().enumerate().filter(|(j, o)| *j != k && o.x0 + 1 >= g.x1).map(|(_, o)| o.x0 as f64 - g.x1 as f64).fold(f64::MAX, f64::min);
            let overlapped = gs.iter().enumerate().any(|(j, o)| j != k && o.x0 < g.x1 && g.x0 < o.x1);
            !((faint || thin) && left > apart && right > apart && !overlapped)
        })
        .collect();
    gs.into_iter().zip(keep).filter(|(_, k)| *k).map(|(g, _)| g).collect()
}

/// One glyph cut from a written line, with where it stood.
#[derive(Debug)]
pub struct Cut {
    pub text: String,
    pub alpha: Gray,
    /// baseline, px from the top of `alpha`
    pub bl: f32,
    /// where it was cut from the letter before / after, in `alpha` px
    pub stubs: [Option<[f32; 2]>; 2],
    /// word number in the line, character number in the word
    pub word: usize,
    pub pos: usize,
    /// the characters written just before and after it in that word
    pub prev: Option<char>,
    pub next: Option<char>,
    /// left edge of `alpha` and the baseline, in line px
    pub x: f32,
    pub y: f32,
}

#[derive(Default, Debug)]
pub struct LineResult {
    pub glyphs: Vec<Cut>,
    pub gaps: Vec<f32>,
    pub words: Vec<f32>,
    pub pairs: HashMap<String, [u32; 2]>,
    pub xh: Vec<f32>,
    pub warnings: Vec<String>,
}

/// Components of `mask`, small ones (dots, accents) attached to the big one they sit over.
fn grouped(mask: &[bool], w: usize, h: usize, min_area: usize, big_h: f32) -> (Vec<Group>, Vec<u32>) {
    let (lab, st) = img::components(mask, w, h);
    let comps: Vec<usize> = (1..st.len()).filter(|&i| st[i].area >= min_area).collect();
    if comps.is_empty() {
        return (vec![], lab);
    }
    let med_area = signal::median(&comps.iter().map(|&i| st[i].area as f32).collect::<Vec<_>>());
    let big: Vec<usize> = comps.iter().copied().filter(|&i| st[i].h() as f32 >= big_h || st[i].area as f32 >= 0.3 * med_area).collect();
    let mut gs: HashMap<usize, Group> = big.iter().map(|&i| (i, Group { labels: vec![i as u32], x0: st[i].x0, x1: st[i].x1, y0: st[i].y0, y1: st[i].y1 })).collect();
    for &c in comps.iter().filter(|c| !big.contains(c)) {
        let s = st[c];
        let mut best = None;
        let mut ov = 0.0;
        for &b in &big {
            let o = (s.x1.min(st[b].x1) as f64 - s.x0.max(st[b].x0) as f64) / (s.w().max(1)) as f64;
            if o > ov {
                ov = o;
                best = Some(b);
            }
        }
        match best {
            // dot, breve, diaeresis: belongs to the letter below/above
            Some(b) if ov >= 0.5 => {
                let g = gs.get_mut(&b).unwrap();
                g.labels.push(c as u32);
                g.x0 = g.x0.min(s.x0);
                g.x1 = g.x1.max(s.x1);
                g.y0 = g.y0.min(s.y0);
                g.y1 = g.y1.max(s.y1);
            }
            _ => {
                gs.insert(c, Group { labels: vec![c as u32], x0: s.x0, x1: s.x1, y0: s.y0, y1: s.y1 });
            }
        }
    }
    let mut v: Vec<Group> = gs.into_values().collect();
    v.sort_by(|a, b| (a.x0 + a.x1).cmp(&(b.x0 + b.x1)));
    (v, lab)
}

fn groups(alpha: &Gray, ppm: f32) -> (Vec<Group>, Vec<u32>) {
    let m: Vec<bool> = alpha.d.iter().map(|&v| v > 0.35).collect();
    let min_area = ((0.03 * ppm * ppm) as usize).max(3);
    // reference height: components that are not specks
    let (_, st) = img::components(&m, alpha.w, alpha.h);
    let areas: Vec<f32> = st.iter().skip(1).map(|c| c.area as f32).collect();
    if areas.is_empty() {
        return (vec![], vec![0; alpha.w * alpha.h]);
    }
    let p30 = signal::percentile(&areas, 30.0);
    let ref_h = signal::median(&st.iter().skip(1).filter(|c| c.area as f32 >= p30).map(|c| c.h() as f32).collect::<Vec<_>>());
    grouped(&m, alpha.w, alpha.h, min_area, 0.5 * ref_h)
}

/// The x-height band of a line: rows where the writing is dense. Returns (top, bottom).
fn core_band(alpha: &Gray, x0: usize, x1: usize, y0: usize, y1: usize) -> Option<(f32, f32)> {
    let rows: Vec<f32> = (y0..y1).map(|y| (x0..x1).map(|x| alpha.d[y * alpha.w + x]).sum()).collect();
    if rows.len() < 3 {
        return None;
    }
    let rows = signal::gauss1d(&rows, 1.0);
    let (imax, &max) = rows.iter().enumerate().max_by(|a, b| a.1.partial_cmp(b.1).unwrap())?;
    if max <= 0.0 {
        return None;
    }
    let t = 0.4 * max;
    let mut a = imax;
    while a > 0 && rows[a - 1] >= t {
        a -= 1;
    }
    let mut b = imax;
    while b + 1 < rows.len() && rows[b + 1] >= t {
        b += 1;
    }
    Some(((y0 + a) as f32, (y0 + b + 1) as f32))
}

/// Components of a word's mask. Dots, apostrophes and accents above the letters go to the
/// letter whose top is nearest (in slanted writing the dot of i sits over the next join);
/// other small pieces go to the letter they overlap, or stand alone (commas, full stops).
/// `apart`: the word holds a tutuq belgisi (taʼlim), so a mark beside a letter rather than
/// over it stays a piece of its own.
fn attach_marks(mask: &[bool], w: usize, h: usize, min_area: usize, xh: f32, bl: &dyn Fn(usize) -> f32, apart: bool) -> (Vec<Group>, Vec<u32>) {
    let (lab, st) = img::components(mask, w, h);
    let comps: Vec<usize> = (1..st.len()).filter(|&i| st[i].area >= min_area).collect();
    // a mark: small and wholly above the letters (i dot written as a tick, apostrophe, accent)
    let is_mark = |i: usize| {
        let c = st[i];
        (c.y1 as f32) < bl(c.x0.midpoint(c.x1)) - 0.7 * xh && (c.area as f32) < 0.35 * xh * xh && (c.h() as f32) < 0.9 * xh
    };
    let big: Vec<usize> = comps.iter().copied().filter(|&i| st[i].h() as f32 >= 0.45 * xh && !is_mark(i)).collect();
    // x-centre of each big piece's ink at the top of the x-height band
    let tops: Vec<Option<f32>> = big
        .iter()
        .map(|&b| {
            let (mut sx, mut n) = (0.0, 0.0);
            for y in st[b].y0..st[b].y1 {
                for x in st[b].x0..st[b].x1 {
                    let base = bl(x);
                    if lab[y * w + x] == b as u32 && (y as f32) >= base - 1.15 * xh && (y as f32) <= base - 0.55 * xh {
                        sx += x as f32;
                        n += 1.0;
                    }
                }
            }
            (n > 0.0).then(|| sx / n)
        })
        .collect();
    let mut gs: HashMap<usize, Group> = big.iter().map(|&i| (i, Group { labels: vec![i as u32], x0: st[i].x0, x1: st[i].x1, y0: st[i].y0, y1: st[i].y1 })).collect();
    for &c in comps.iter().filter(|c| !big.contains(c)) {
        let s = st[c];
        let target = if is_mark(c) {
            let reach = if apart { 0.35 } else { 0.8 } * xh;
            big.iter().zip(&tops).filter_map(|(&b, t)| t.map(|t| (b, (t - s.cx).abs()))).filter(|(_, d)| *d < reach).min_by(|a, b| a.1.partial_cmp(&b.1).unwrap()).map(|x| x.0)
        } else {
            big.iter().copied().map(|b| (b, (s.x1.min(st[b].x1) as f64 - s.x0.max(st[b].x0) as f64) / s.w().max(1) as f64)).filter(|x| x.1 >= 0.5).max_by(|a, b| a.1.partial_cmp(&b.1).unwrap()).map(|x| x.0)
        };
        match target {
            Some(b) => {
                let g = gs.get_mut(&b).unwrap();
                g.labels.push(c as u32);
                g.x0 = g.x0.min(s.x0);
                g.x1 = g.x1.max(s.x1);
                g.y0 = g.y0.min(s.y0);
                g.y1 = g.y1.max(s.y1);
            }
            None => {
                gs.insert(c, Group { labels: vec![c as u32], x0: s.x0, x1: s.x1, y0: s.y0, y1: s.y1 });
            }
        }
    }
    let mut v: Vec<Group> = gs.into_values().collect();
    v.sort_by(|a, b| (a.x0 + a.x1).cmp(&(b.x0 + b.x1)));
    (v, lab)
}

/// One piece of a word after cutting joined letters apart.
#[derive(Clone, Debug)]
struct Prim {
    labels: Vec<u32>,
    x0: usize,
    x1: usize,
    asc: bool,
    desc: bool,
}

/// A word's ink cut into pieces, in word-box coordinates.
struct Word {
    ox: usize,
    oy: usize,
    w: usize,
    h: usize,
    lab: Vec<u32>,
    prims: Vec<Prim>,
    /// (x, y) where piece k and k + 1 were cut apart (None: they did not touch)
    cut_after: Vec<Option<[f32; 2]>>,
}

fn split_word(alpha: &Gray, lab: &[u32], wg: &[Group], bl: &dyn Fn(f32) -> f32, xh: f32, pen: f32, min_area: usize, apart: bool) -> Word {
    let ox = wg.iter().map(|g| g.x0).min().unwrap();
    let x1 = wg.iter().map(|g| g.x1).max().unwrap();
    let oy = wg.iter().map(|g| g.y0).min().unwrap();
    let y1 = wg.iter().map(|g| g.y1).max().unwrap();
    let (w, h) = (x1 - ox, y1 - oy);
    let labels: HashSet<u32> = wg.iter().flat_map(|g| g.labels.iter().copied()).collect();
    let mut mask: Vec<bool> = (0..w * h).map(|i| labels.contains(&lab[(oy + i / w) * alpha.w + ox + i % w])).collect();
    // columns crossed by exactly one thin stroke near the baseline: the joins between letters
    let max_run = (1.9 * pen).max(0.3 * xh);
    let mut cand: Vec<Option<(usize, usize)>> = vec![None; w];
    for x in 0..w {
        let mut runs = vec![];
        let mut y = 0;
        while y < h {
            if mask[y * w + x] {
                let s = y;
                while y < h && mask[y * w + x] {
                    y += 1;
                }
                runs.push((s, y));
            } else {
                y += 1;
            }
        }
        // only strokes where letters join count: loops of tall letters leaning over the
        // next letter, and descender loops under the previous one, pass above / below
        let b = bl((ox + x) as f32) - oy as f32;
        runs.retain(|&(s, e)| (e as f32) > b - 0.85 * xh && (s as f32) < b + 0.25 * xh);
        if let [(s, e)] = runs[..] {
            let c = (s + e) as f32 / 2.0;
            if ((e - s) as f32) <= max_run && c >= b - 0.75 * xh && c <= b + 0.15 * xh {
                cand[x] = Some((s, e));
            }
        }
    }
    let edge = (0.25 * xh) as usize;
    let mut cuts: Vec<(usize, usize, usize)> = vec![];
    let mut x = 0;
    while x < w {
        if cand[x].is_some() {
            let s = x;
            while x < w && cand[x].is_some() {
                x += 1;
            }
            let m = (s + x - 1) / 2;
            if m > edge && m + edge < w && cuts.last().is_none_or(|c| m - c.0 > edge) {
                let (y0, y1) = cand[m].unwrap();
                cuts.push((m, y0, y1));
            }
        } else {
            x += 1;
        }
    }
    for &(cx, y0, y1) in &cuts {
        for y in y0.saturating_sub(1)..(y1 + 1).min(h) {
            mask[y * w + cx] = false;
        }
    }
    let (groups, wlab) = attach_marks(&mask, w, h, min_area.min(4), xh, &|x| bl((ox + x) as f32) - oy as f32, apart);
    let mut prims: Vec<Prim> = groups
        .into_iter()
        .map(|g| {
            let (mut up, mut down) = (0, 0);
            for y in g.y0..g.y1 {
                for x in g.x0..g.x1 {
                    if g.labels.contains(&wlab[y * w + x]) {
                        let b = bl((ox + x) as f32) - oy as f32;
                        if (y as f32) < b - 1.4 * xh {
                            up += 1;
                        }
                        if (y as f32) > b + 0.5 * xh {
                            down += 1;
                        }
                    }
                }
            }
            let enough = (0.25 * xh).max(4.0) as usize;
            Prim { labels: g.labels, x0: g.x0, x1: g.x1, asc: up >= enough, desc: down >= enough }
        })
        .collect();
    prims.sort_by(|a, b| (a.x0 + a.x1).cmp(&(b.x0 + b.x1)));
    // which neighbouring pieces a cut separated
    let owner = |x: usize, y: usize| -> Option<usize> {
        let l = wlab[y * w + x];
        prims.iter().position(|p| p.labels.contains(&l))
    };
    let mut cut_after = vec![None; prims.len().saturating_sub(1)];
    for &(cx, y0, y1) in &cuts {
        let ym = (y0 + y1) / 2;
        let left = (1..4).filter_map(|d| cx.checked_sub(d)).find_map(|x| (y0.saturating_sub(1)..(y1 + 1).min(h)).find_map(|y| owner(x, y)).or_else(|| owner(x, ym)));
        let right = (1..4).map(|d| cx + d).filter(|&x| x < w).find_map(|x| (y0.saturating_sub(1)..(y1 + 1).min(h)).find_map(|y| owner(x, y)));
        if let (Some(a), Some(b)) = (left, right) {
            if b == a + 1 {
                cut_after[a] = Some([cx as f32, ym as f32]);
            }
        }
    }
    Word { ox, oy, w, h, lab: wlab, prims, cut_after }
}

/// Pieces k0..k1 written as characters i0..i1.
#[derive(Clone, Copy, Debug)]
struct Seg {
    k0: usize,
    k1: usize,
    i0: usize,
    i1: usize,
    cost: f64,
    /// width / (unit * expected weight)
    ratio: f64,
    /// width in units of the word's average letter
    width: f64,
    /// ascender / descender mismatch
    feat: f64,
}

/// Segments grouping pieces into characters, covering both. `look`: how much pieces k..k+m
/// look like a character (None when its typical shapes are not known yet).
fn align(word: &Word, chars: &[char], widths: &Widths, look: &dyn Fn(usize, usize, char) -> Option<f32>) -> Vec<Seg> {
    let ps = &word.prims;
    let (g, c) = (ps.len(), chars.len());
    if g == 0 || c == 0 {
        return vec![];
    }
    // piece boundaries along x: the cut, or the middle of the gap
    let mut bounds = vec![ps[0].x0 as f64];
    for k in 0..g - 1 {
        let b = word.cut_after[k].map(|c| c[0] as f64).unwrap_or_else(|| {
            let (r, l) = (ps[k].x1 as f64, ps[k + 1].x0 as f64);
            if l >= r { (l + r) / 2.0 } else { (ps[k].x0 + ps[k].x1 + ps[k + 1].x0 + ps[k + 1].x1) as f64 / 4.0 }
        });
        bounds.push(b.max(*bounds.last().unwrap()));
    }
    bounds.push((ps[g - 1].x1 as f64).max(*bounds.last().unwrap()));
    let wts: Vec<f64> = chars.iter().map(|&ch| widths.weight(ch)).collect();
    let unit = (bounds[g] - bounds[0]) / wts.iter().sum::<f64>().max(1e-6);
    let feature = |k0: usize, k1: usize, cs: &[char]| -> f64 {
        let (asc, desc) = (ps[k0..k1].iter().any(|p| p.asc), ps[k0..k1].iter().any(|p| p.desc));
        let mut cost = 0.0;
        let ea: Vec<bool> = cs.iter().filter_map(|&ch| expects_ascender(ch)).collect();
        if !ea.is_empty() && ea.iter().any(|&e| e) != asc {
            cost += 0.9;
        }
        let ed: Vec<bool> = cs.iter().filter_map(|&ch| expects_descender(ch)).collect();
        if !ed.is_empty() && ed.iter().any(|&e| e) != desc {
            cost += 0.9;
        }
        cost
    };
    let inf = f64::INFINITY;
    let mut dp = vec![vec![inf; c + 1]; g + 1];
    let mut back: HashMap<(usize, usize), Seg> = HashMap::new();
    dp[0][0] = 0.0;
    for k in 0..g {
        for i in 0..c {
            let cur = dp[k][i];
            if !cur.is_finite() {
                continue;
            }
            for n in 1..=3usize.min(c - i) {
                let extra = chars[i + 1..i + n].iter().filter(|ch| !is_mark(**ch)).count();
                for m in 1..=6usize.min(g - k) {
                    let width = (bounds[k + m] - bounds[k]).max(1.0);
                    let exp = (unit * wts[i..i + n].iter().sum::<f64>()).max(1.0);
                    let feat = feature(k, k + m, &chars[i..i + n]);
                    let cost = (width / exp).ln().powi(2) + 0.7 * extra as f64 + 0.03 * (m - 1) as f64 + feat;
                    // a single letter that does not look like the person's: a wrong grouping
                    let unlike = if n == 1 { look(k, m, chars[i]).map_or(0.0, |s| 2.0 * (0.8 - s as f64).max(0.0)) } else { 0.0 };
                    if cur + cost + unlike < dp[k + m][i + n] {
                        dp[k + m][i + n] = cur + cost + unlike;
                        back.insert((k + m, i + n), Seg { k0: k, k1: k + m, i0: i, i1: i + n, cost, ratio: width / exp, width: width / unit.max(1e-6), feat });
                    }
                }
            }
        }
    }
    if !dp[g][c].is_finite() {
        return vec![];
    }
    let mut segs = vec![];
    let mut cur = (g, c);
    while cur != (0, 0) {
        let sg = back[&cur];
        segs.push(sg);
        cur = (sg.k0, sg.i0);
    }
    segs.reverse();
    segs
}

/// Where the words of a line start, as piece indices (plus the end). Breaks must fall into
/// gaps between pieces; each word should be about as wide as its letters, and a break in a
/// gap much narrower than the typical word gap costs extra.
fn word_breaks(gs: &[Group], words: &[Vec<char>]) -> Option<Vec<usize>> {
    let (g, n) = (gs.len(), words.len());
    if n == 1 {
        return Some(vec![0, g]);
    }
    let mut right = vec![0usize; g];
    for i in 0..g {
        right[i] = gs[i].x1.max(if i > 0 { right[i - 1] } else { 0 });
    }
    // gap before piece k (k >= 1)
    let gap = |k: usize| (gs[k].x0 as f64 - right[k - 1] as f64).max(0.5);
    let mut sorted: Vec<f64> = (1..g).map(gap).collect();
    sorted.sort_by(|a, b| b.partial_cmp(a).unwrap());
    let typical = sorted[(n - 2).min(sorted.len() - 1)].max(1.0);
    let ww: Vec<f64> = words.iter().map(|w| w.iter().map(|&c| char_weight(c)).sum()).collect();
    let span = (right[g - 1] - gs[0].x0) as f64;
    let unit = span / (ww.iter().sum::<f64>() + 0.6 * (n - 1) as f64).max(1e-6);
    let inf = f64::INFINITY;
    let mut dp = vec![vec![inf; n + 1]; g + 1];
    let mut back = vec![vec![0usize; n + 1]; g + 1];
    dp[0][0] = 0.0;
    for k in 0..g {
        for i in 0..n {
            if !dp[k][i].is_finite() {
                continue;
            }
            let brk = if k == 0 { 0.0 } else { (typical / gap(k)).ln().max(0.0) * 1.5 };
            for e in k + 1..=g {
                let width = (right[e - 1] - gs[k].x0) as f64;
                let cost = dp[k][i] + brk + (width.max(1.0) / (unit * ww[i]).max(1.0)).ln().powi(2);
                if cost < dp[e][i + 1] {
                    dp[e][i + 1] = cost;
                    back[e][i + 1] = k;
                }
            }
        }
    }
    if !dp[g][n].is_finite() {
        return None;
    }
    let mut b = vec![g];
    let mut e = g;
    for i in (1..=n).rev() {
        e = back[e][i];
        b.push(e);
    }
    b.reverse();
    Some(b)
}

pub fn harvest_line(alpha: &Gray, text: &str, ppm: f32, max_cost: f64) -> LineResult {
    harvest(alpha, text, ppm, max_cost, &Widths::default(), None)
}

/// First pass: measure this person's letter widths on a line (letters cut cleanly only).
pub fn learn_widths(alpha: &Gray, text: &str, ppm: f32, widths: &mut Widths) {
    let mut seen = vec![];
    let prior = Widths::default();
    harvest(alpha, text, ppm, 0.0, &prior, Some(&mut seen));
    for (c, w) in seen {
        widths.seen.entry(c).or_default().push(w);
    }
}

/// Cut a line into glyphs. With `learn`, nothing is kept: single letters that fit well are
/// reported with their measured widths instead.
pub fn harvest(alpha: &Gray, text: &str, ppm: f32, max_cost: f64, widths: &Widths, mut learn: Option<&mut Vec<(char, f64)>>) -> LineResult {
    let mut res = LineResult::default();
    let text = charset::normalize(text);
    let words: Vec<Vec<char>> = text.split_whitespace().map(|w| w.chars().collect()).collect();
    let (gs, lab) = groups(alpha, ppm);
    let gs = drop_stray(gs, &lab, alpha, ppm);
    if gs.is_empty() || words.is_empty() {
        return res;
    }
    if gs.len() < words.len() {
        res.warnings.push(format!("too few ink pieces for: {text}"));
        return res;
    }
    let Some(bounds) = word_breaks(&gs, &words) else {
        res.warnings.push(format!("could not find the words of: {text}"));
        return res;
    };
    for wi in 1..words.len() {
        let right = gs[..bounds[wi]].iter().map(|g| g.x1).max().unwrap();
        res.words.push(gs[bounds[wi]].x0 as f32 - right as f32);
    }

    // Baseline: bottom of each word's x-height band, fitted as a line; x-height: the band.
    let mut bx = vec![];
    let mut by = vec![];
    let mut hs = vec![];
    for wi in 0..words.len() {
        let wg = &gs[bounds[wi]..bounds[wi + 1]];
        let (x0, x1) = (wg.iter().map(|g| g.x0).min().unwrap(), wg.iter().map(|g| g.x1).max().unwrap());
        let (y0, y1) = (wg.iter().map(|g| g.y0).min().unwrap(), wg.iter().map(|g| g.y1).max().unwrap());
        if let Some((t, b)) = core_band(alpha, x0, x1, y0, y1) {
            bx.push((x0 + x1) as f64 / 2.0);
            by.push(b as f64);
            hs.push(b - t);
        }
    }
    if hs.is_empty() {
        return res;
    }
    let xh = signal::median(&hs).max(3.0);
    let (a, b) = if bx.len() >= 3 {
        let c = signal::polyfit(&bx, &by, 1);
        (c[0], c[1])
    } else {
        (0.0, signal::median(&by.iter().map(|&v| v as f32).collect::<Vec<_>>()) as f64)
    };
    let bl = move |x: f32| (a * x as f64 + b) as f32;
    let pen = crate::hand::features(alpha).sw;
    let min_area = ((0.03 * ppm * ppm) as usize).max(3);

    // letters per x-height of each word: a word far wider or narrower per letter than the
    // others on its line was matched to the wrong ink
    let per_letter: Vec<Option<f32>> = words
        .iter()
        .enumerate()
        .map(|(wi, word)| {
            let wg = &gs[bounds[wi]..bounds[wi + 1]];
            let span = (wg.iter().map(|g| g.x1).max()? - wg.iter().map(|g| g.x0).min()?) as f32;
            let w: f64 = word.iter().map(|&c| widths.weight(c)).sum();
            (!word.iter().all(|&c| c == '#') && word.len() >= 2).then(|| span / (w as f32).max(0.5) / xh)
        })
        .collect();
    let typical = {
        let v: Vec<f32> = per_letter.iter().flatten().copied().collect();
        (v.len() >= 3).then(|| signal::median(&v))
    };
    for (wi, word) in words.iter().enumerate() {
        // "#": ink that is not text (a crossed-out word, a scribble): skipped
        if word.iter().all(|&c| c == '#') {
            continue;
        }
        if let (Some(t), Some(u)) = (typical, per_letter[wi]) {
            if u > 1.7 * t || u < 0.55 * t {
                res.warnings.push(format!("could not align: {}", word.iter().collect::<String>()));
                continue;
            }
        }
        let wg = &gs[bounds[wi]..bounds[wi + 1]];
        let wd = split_word(alpha, &lab, wg, &bl, xh, pen, min_area, word.contains(&charset::TUTUQ));
        // what each grouping looks like, worked out once
        let cache: std::cell::RefCell<HashMap<(usize, usize), Vec<f32>>> = Default::default();
        let look = |k: usize, m: usize, c: char| -> Option<f32> {
            widths.shapes.get(&c).filter(|p| p.len() >= 3)?;
            let mut cache = cache.borrow_mut();
            let sh = cache.entry((k, m)).or_insert_with(|| seg_shape(alpha, &wd, k, k + m, &bl, xh));
            shape_match(widths, c, sh)
        };
        let segs = align(&wd, word, widths, &look);
        if segs.is_empty() {
            res.warnings.push(format!("could not align: {}", word.iter().collect::<String>()));
            continue;
        }
        if std::env::var("HW_DEBUG_CUTS").is_ok() && learn.is_none() {
            let parts: Vec<String> = segs.iter().map(|s| format!("{}[{}p r{:.2} f{:.1} c{:.2}]", word[s.i0..s.i1].iter().collect::<String>(), s.k1 - s.k0, s.ratio, s.feat, s.cost)).collect();
            eprintln!("{}: {}", word.iter().collect::<String>(), parts.join(" "));
        }
        if let Some(l) = learn.as_mut() {
            for sg in &segs {
                if sg.i1 - sg.i0 == 1 && sg.feat == 0.0 && sg.ratio.ln().abs() < 1.1 && word.len() >= 3 {
                    l.push((word[sg.i0], sg.width));
                }
            }
            continue;
        }
        for (si, sg) in segs.iter().enumerate() {
            let Seg { k0, k1, i0, i1, cost, .. } = *sg;
            let chunk: String = word[i0..i1].iter().collect();
            let pieces = &wd.prims[k0..k1];
            // letters within one piece were written joined
            let joined_inside = i1 - i0 > 1 && k1 - k0 == 1;
            if joined_inside {
                for j in i0..i1 - 1 {
                    let e = res.pairs.entry(word[j..j + 2].iter().collect()).or_insert([0, 0]);
                    e[0] += 1;
                    e[1] += 1;
                }
            }
            let x1 = pieces.iter().map(|g| g.x1).max().unwrap();
            if let Some(next) = segs.get(si + 1) {
                let pair: String = [word[i1 - 1], word[next.i0]].iter().collect();
                let e = res.pairs.entry(pair).or_insert([0, 0]);
                e[1] += 1;
                if wd.cut_after[k1 - 1].is_some() {
                    e[0] += 1;
                } else {
                    res.gaps.push(wd.prims[next.k0].x0 as f32 - x1 as f32);
                }
            }
            let extra = word[i0 + 1..i1].iter().filter(|c| !is_mark(**c)).count();
            // a capital starting a word is drawn wider and freer than the letters after it: its
            // width says less about where it ends
            let limit = if i0 == 0 && i1 - i0 == 1 && word[0].is_uppercase() { max_cost.max(0.6) } else { max_cost };
            // the tutuq belgisi is never kept stuck to a letter
            if cost > limit || extra > 1 || (i1 - i0 > 1 && chunk.contains(charset::TUTUQ)) {
                continue;
            }
            // (capitals vary too much for it: a cursive T looks nothing like a printed one)
            if i1 - i0 == 1 && !word[i0].is_uppercase() && look(k0, k1 - k0, word[i0]).is_some_and(|s| s < 0.45) {
                continue;
            }
            let labels: HashSet<u32> = pieces.iter().flat_map(|g| g.labels.iter().copied()).collect();
            let x0 = pieces.iter().map(|g| g.x0).min().unwrap();
            let (mut y0, mut y1) = (usize::MAX, 0);
            for y in 0..wd.h {
                for x in x0..x1 {
                    if labels.contains(&wd.lab[y * wd.w + x]) {
                        y0 = y0.min(y);
                        y1 = y1.max(y + 1);
                    }
                }
            }
            if y0 == usize::MAX {
                continue;
            }
            // no letter is that wide or tall: a wrong cut (a whole word, a table line)
            let (gw, gh) = ((x1 - x0) as f32 / xh, (y1 - y0) as f32 / xh);
            let c0 = chunk.chars().next().unwrap();
            let wide = "mwшщжюыMWМШЩЖЮЫ".contains(c0) || c0 == '—';
            let max_w = if chunk.chars().count() > 1 {
                1.8 * chunk.chars().count() as f32
            } else if c0.is_uppercase() {
                // a written capital swings wide with its loops
                if wide { 5.2 } else { 4.2 }
            } else if wide {
                3.0
            } else if "il1.,:;!'ʻʼ’|()[]ıj-".contains(c0) {
                1.4
            } else {
                2.2
            };
            // and a letter is never just a dot (an i dot or a full stop filed as a letter); a
            // capital may stand far above the small letters
            let max_h = if chunk.chars().count() == 1 && c0.is_uppercase() { 5.0 } else { 4.0 };
            if gw > max_w || gh > max_h || (c0.is_alphanumeric() && gh < 0.55) {
                continue;
            }
            let pad = (0.6 * ppm) as usize;
            let (cx0, cx1) = (x0.saturating_sub(pad), (x1 + pad).min(wd.w));
            let (cy0, cy1) = (y0.saturating_sub(pad), (y1 + pad).min(wd.h));
            let (cw, ch) = (cx1 - cx0, cy1 - cy0);
            let mut keep = vec![false; cw * ch];
            for y in 0..ch {
                for x in 0..cw {
                    keep[y * cw + x] = labels.contains(&wd.lab[(cy0 + y) * wd.w + cx0 + x]);
                }
            }
            // one pixel more: the ink under the cut belongs to both sides
            let keep = img::dilate_mask(&keep, cw, ch, 1);
            let mut crop = alpha.crop(wd.ox + cx0, wd.oy + cy0, wd.ox + cx1, wd.oy + cy1);
            for (v, k) in crop.d.iter_mut().zip(&keep) {
                if !k {
                    *v = 0.0;
                }
            }
            let cx = wd.ox as f32 + (x0 + x1) as f32 / 2.0;
            let base = bl(cx) - (wd.oy + cy0) as f32;
            if crop.d.iter().filter(|&&v| v > 0.3).count() > 5 {
                if chunk.chars().count() == 1 && charset::X_HEIGHT.contains(&chunk.chars().next().unwrap()) {
                    res.xh.push(bl(cx) - (wd.oy + y0) as f32);
                }
                // the cut ends of its joining strokes: where it joins its neighbours again
                let at = |c: Option<[f32; 2]>| c.map(|c| [c[0] - cx0 as f32, c[1] - cy0 as f32]);
                let entry = if k0 > 0 { at(wd.cut_after[k0 - 1]) } else { None };
                let exit = at(wd.cut_after.get(k1 - 1).copied().flatten());
                res.glyphs.push(Cut {
                    text: chunk,
                    alpha: crop,
                    bl: base,
                    stubs: [entry, exit],
                    word: wi,
                    pos: i0,
                    prev: i0.checked_sub(1).map(|j| word[j]),
                    next: word.get(i1).copied(),
                    x: (wd.ox + cx0) as f32,
                    y: bl(cx),
                });
            }
        }
    }
    res
}

/// Straighten a page whose lines run at a slant (a tilted photo): the vertical shear that
/// makes the rows of ink sharpest, within ±6°.
pub fn deskew(alpha: &Gray) -> Gray {
    let (w, h) = (alpha.w, alpha.h);
    let cx = w as f32 / 2.0;
    let pts: Vec<(f32, f32, f32)> = (0..h)
        .step_by(2)
        .flat_map(|y| (0..w).step_by(2).map(move |x| (x, y)))
        .filter_map(|(x, y)| {
            let v = alpha.d[y * w + x];
            (v > 0.3).then_some((x as f32 - cx, y as f32, v))
        })
        .collect();
    if pts.len() < 50 {
        return alpha.clone();
    }
    let score = |deg: f32| -> f32 {
        let t = deg.to_radians().tan();
        let pad = (cx * 0.12) as usize + 2;
        let mut bins = vec![0f32; h + 2 * pad];
        for &(x, y, v) in &pts {
            let yy = (y - x * t).round() as i64 + pad as i64;
            if yy >= 0 && (yy as usize) < bins.len() {
                bins[yy as usize] += v;
            }
        }
        let b = signal::gauss1d(&bins, 2.0);
        b.iter().map(|v| v * v).sum()
    };
    let best = (-30..=30).map(|k| k as f32 * 0.2).max_by(|a, b| score(*a).partial_cmp(&score(*b)).unwrap()).unwrap_or(0.0);
    if best.abs() < 0.3 {
        return alpha.clone();
    }
    let t = best.to_radians().tan();
    let mut out = Gray::new(w, h, 0.0);
    for y in 0..h {
        for x in 0..w {
            out.d[y * w + x] = alpha.sample(x as f32, y as f32 + (x as f32 - cx) * t);
        }
    }
    out
}

/// Every text line on a page matte, top to bottom: rows of ink, generously found (a stray
/// mark or page number is a line too; matching to the transcript sorts them out).
pub fn find_lines(alpha: &Gray, ppm: f32) -> Vec<Gray> {
    let rows: Vec<f32> = (0..alpha.h).map(|y| alpha.d[y * alpha.w..(y + 1) * alpha.w].iter().sum()).collect();
    let prof = signal::gauss1d(&rows, ppm * 0.8);
    let max = prof.iter().cloned().fold(0.0, f32::max);
    let mut peaks = signal::find_peaks(&prof, ((3.0 * ppm) as usize).max(2), max * 0.03);
    // the usual line spacing, from the clear lines
    let mut main: Vec<usize> = peaks.iter().filter(|p| p.1 >= 0.25 * max).map(|p| p.0).collect();
    main.sort();
    let pitch = if main.len() >= 3 { signal::median(&main.windows(2).map(|w| (w[1] - w[0]) as f32).collect::<Vec<_>>()) } else { 8.0 * ppm };
    // a line's tall or hanging letters can make a second, weaker peak: keep the stronger one
    peaks.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    let mut kept: Vec<usize> = vec![];
    for (y, _) in peaks {
        if kept.iter().all(|&k| (k as f32 - y as f32).abs() >= 0.55 * pitch) {
            kept.push(y);
        }
    }
    bands(alpha, &prof, kept, ppm)
}

/// The width of a row of writing (px): the span of its words. Blobs much smaller than its
/// biggest word (specks, bits of a printed line, a margin rule) do not count, so a short line
/// does not look full.
pub fn row_width(g: &Gray) -> f64 {
    let (a, b) = row_span(g);
    (b - a).max(0.0)
}

/// Where a row's words start and end (px); (0, 0) for an empty row.
pub fn row_span(g: &Gray) -> (f64, f64) {
    let (w, h) = (g.w, g.h);
    let on: Vec<bool> = g.d.iter().map(|&v| v > 0.35).collect();
    if !on.iter().any(|&b| b) {
        return (0.0, 0.0);
    }
    // letters of one word (and its dots) into one blob
    let r = (h / 8).max(2);
    let wide: Vec<bool> = (0..w * h)
        .map(|i| {
            let (x, y) = (i % w, i / w);
            (x.saturating_sub(r)..(x + r + 1).min(w)).any(|xx| on[y * w + xx])
        })
        .collect();
    let (lab, comps) = img::components(&wide, w, h);
    let mut mass = vec![0.0f64; comps.len()];
    for (i, &l) in lab.iter().enumerate() {
        if l != 0 && on[i] {
            mass[l as usize] += g.d[i] as f64;
        }
    }
    let biggest = mass.iter().cloned().fold(0.0, f64::max);
    let words: Vec<usize> = (1..comps.len()).filter(|&k| mass[k] >= 0.1 * biggest).collect();
    let x0 = words.iter().map(|&k| comps[k].x0).min().unwrap_or(0) as f64;
    let x1 = words.iter().map(|&k| comps[k].x1).max().unwrap_or(0) as f64;
    // the dilation widened each blob by r on both sides
    let (a, b) = (x0 + r as f64, x1 - r as f64);
    (a, b.max(a + 1.0))
}

/// Pair found lines with transcript lines by width (dynamic programming): a found line may
/// be noise, a transcript line may not have been found, and a line that did not fit may go
/// on over two rows (split between words). Returns (found, transcript line, its text on that
/// row) and the total cost (lower is a better fit).
pub fn match_lines(found: &[Gray], text: &[&str]) -> (Vec<(usize, usize, String)>, f64) {
    let spans: Vec<(f64, f64)> = found.iter().map(row_span).collect();
    match_widths(&spans, &[], text)
}

/// `match_lines` on where each row's writing starts and ends. `starts`: rows that begin a new
/// photo of the same page, where a line may always go on (the sheet ended there).
pub fn match_widths(spans: &[(f64, f64)], starts: &[usize], text: &[&str]) -> (Vec<(usize, usize, String)>, f64) {
    let ws: Vec<f64> = spans.iter().map(|s| (s.1 - s.0).max(0.0)).collect();
    let weight = |t: &str| -> f64 { charset::normalize(t).chars().map(|c| if c == ' ' { 1.0 } else { char_weight(c) }).sum::<f64>().max(0.5) };
    let ts: Vec<f64> = text.iter().map(|t| weight(t)).collect();
    let words: Vec<Vec<&str>> = text.iter().map(|t| t.split_whitespace().collect()).collect();
    let unit = ws.iter().sum::<f64>() / ts.iter().sum::<f64>().max(1e-6);
    let med = {
        let mut v = ws.to_vec();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v.get(v.len() / 2).copied().unwrap_or(1.0)
    };
    let fit = |w: f64, t: f64| (w.max(1.0) / (unit * t).max(1.0)).ln().powi(2);
    let left = {
        let mut v: Vec<f64> = spans.iter().filter(|s| s.1 - s.0 >= 0.5 * med).map(|s| s.0).collect();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v.get(v.len() / 2).copied().unwrap_or(0.0)
    };
    let at_start = |k: usize| ws[k] > 0.0 && (spans[k].0 - left).abs() < 0.06 * med;
    // a line goes on to the next row only when it is longer than the rows are wide
    let full = {
        let mut v = ws.to_vec();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v.get(v.len() * 9 / 10).copied().unwrap_or(0.0)
    };
    if std::env::var("HW_DEBUG_CUTS").is_ok() {
        eprintln!("full {full:.0} unit {unit:.2}; rows {:?}; lines {:?}", ws.iter().map(|w| (w / full * 100.0) as i32).collect::<Vec<_>>(), ts.iter().map(|t| (unit * t / full * 100.0) as i32).collect::<Vec<_>>());
    }
    let (f, t) = (ws.len(), ts.len());
    let inf = f64::INFINITY;
    let mut dp = vec![vec![inf; t + 1]; f + 1];
    // previous state and, for a line over two rows, the number of words on the first
    let mut back = vec![vec![(0usize, 0usize, 0usize); t + 1]; f + 1];
    dp[0][0] = 0.0;
    for k in 0..=f {
        for i in 0..=t {
            let cur = dp[k][i];
            if !cur.is_finite() {
                continue;
            }
            let mut step = |nk: usize, ni: usize, c: f64, split: usize| {
                if cur + c < dp[nk][ni] {
                    dp[nk][ni] = cur + c;
                    back[nk][ni] = (k, i, split);
                }
            };
            if k < f && i < t {
                step(k + 1, i + 1, fit(ws[k], ts[i]), 0);
            }
            // a line goes on to the next row when it is longer than the rows are wide, when the
            // photo ends, or when the next row is a few words starting where lines start
            if k + 1 < f && i < t && words[i].len() >= 2 && (unit * ts[i] > 1.3 * full || starts.contains(&(k + 1)) || (ws[k + 1] < 0.5 * med && at_start(k + 1))) {
                // a line written on two rows: the best place to break it
                let best = (1..words[i].len())
                    .map(|s| (fit(ws[k], weight(&words[i][..s].join(" "))) + fit(ws[k + 1], weight(&words[i][s..].join(" "))), s))
                    .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
                if let Some((c, s)) = best {
                    step(k + 2, i + 1, c + 0.45, s);
                }
            }
            if k < f {
                // a found line that is not text: cheap when it is small
                // a found line that is not text: cheap when it is small and away from where
                // lines start (a page number, a speck); a few words at the start are text
                step(k + 1, i, if ws[k] < 0.15 * med && !at_start(k) { 0.3 } else if ws[k] < 0.15 * med { 1.0 } else { 2.0 }, 0);
            }
            if i < t {
                step(k, i + 1, 2.5, 0);
            }
        }
    }
    let mut pairs = vec![];
    let mut cur = (f, t);
    while cur != (0, 0) {
        let (pk, pi, split) = back[cur.0][cur.1];
        if pi + 1 == cur.1 && pk + 2 == cur.0 && split > 0 {
            pairs.push((pk + 1, pi, words[pi][split..].join(" ")));
            pairs.push((pk, pi, words[pi][..split].join(" ")));
        } else if pk + 1 == cur.0 && pi + 1 == cur.1 {
            pairs.push((pk, pi, text[pi].to_string()));
        }
        cur = (pk, pi);
    }
    pairs.reverse();
    (pairs, dp[f][t])
}

/// Line mattes around the given row centres: ink pieces go to the band their centre is in.
fn bands(alpha: &Gray, prof: &[f32], mut centers: Vec<usize>, ppm: f32) -> Vec<Gray> {
    if centers.is_empty() {
        return vec![];
    }
    centers.sort();
    let pitch = if centers.len() >= 2 {
        signal::median(&centers.windows(2).map(|w| (w[1] - w[0]) as f32).collect::<Vec<_>>()) as usize
    } else {
        (12.0 * ppm) as usize
    };
    let mut cuts = vec![centers[0].saturating_sub(pitch)];
    for i in 0..centers.len() - 1 {
        let (a, b) = (centers[i], centers[i + 1]);
        let m = (a..b).min_by(|&x, &y| prof[x].partial_cmp(&prof[y]).unwrap()).unwrap_or(a);
        cuts.push(m);
    }
    cuts.push((centers[centers.len() - 1] + pitch).min(alpha.h));
    let m: Vec<bool> = alpha.d.iter().map(|&v| v > 0.35).collect();
    let (lab, st) = img::components(&m, alpha.w, alpha.h);
    let mut out = vec![];
    for i in 0..centers.len() {
        let ids: Vec<u32> = (1..st.len()).filter(|&j| (cuts[i] as f32) <= st[j].cy && st[j].cy < cuts[i + 1] as f32).map(|j| j as u32).collect();
        if ids.is_empty() {
            out.push(Gray::new(1, 1, 0.0));
            continue;
        }
        let y0 = ids.iter().map(|&j| st[j as usize].y0).min().unwrap();
        let y1 = ids.iter().map(|&j| st[j as usize].y1).max().unwrap();
        let mut sel = vec![false; alpha.w * (y1 - y0)];
        for y in y0..y1 {
            for x in 0..alpha.w {
                sel[(y - y0) * alpha.w + x] = ids.contains(&lab[y * alpha.w + x]);
            }
        }
        let sel = img::dilate_mask(&sel, alpha.w, y1 - y0, 1);
        let mut g = alpha.crop(0, y0, alpha.w, y1);
        for (v, k) in g.d.iter_mut().zip(&sel) {
            if !k {
                *v = 0.0;
            }
        }
        out.push(g);
    }
    out
}

/// Split a page matte into `n` text lines (n from the transcript).
pub fn split_lines(alpha: &Gray, n: usize, ppm: f32) -> Vec<Gray> {
    let rows: Vec<f32> = (0..alpha.h).map(|y| alpha.d[y * alpha.w..(y + 1) * alpha.w].iter().sum()).collect();
    let prof = signal::gauss1d(&rows, ppm * 0.8);
    let max = prof.iter().cloned().fold(0.0, f32::max);
    let mut peaks = signal::find_peaks(&prof, ((4.0 * ppm) as usize).max(2), max * 0.05);
    if peaks.is_empty() {
        return vec![];
    }
    peaks.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    let mut centers: Vec<usize> = peaks.iter().take(n).map(|p| p.0).collect();
    centers.sort();
    // the first and last lines reach as far as the others do, not to the page edge
    let pitch = if centers.len() >= 2 {
        signal::median(&centers.windows(2).map(|w| (w[1] - w[0]) as f32).collect::<Vec<_>>()) as usize
    } else {
        (12.0 * ppm) as usize
    };
    let mut cuts = vec![centers[0].saturating_sub(pitch)];
    for i in 0..centers.len() - 1 {
        let (a, b) = (centers[i], centers[i + 1]);
        let m = (a..b).min_by(|&x, &y| prof[x].partial_cmp(&prof[y]).unwrap()).unwrap_or(a);
        cuts.push(m);
    }
    cuts.push((centers[centers.len() - 1] + pitch).min(alpha.h));
    let m: Vec<bool> = alpha.d.iter().map(|&v| v > 0.35).collect();
    let (lab, st) = img::components(&m, alpha.w, alpha.h);
    let mut out = vec![];
    for i in 0..centers.len() {
        let ids: Vec<u32> = (1..st.len()).filter(|&j| (cuts[i] as f32) <= st[j].cy && st[j].cy < cuts[i + 1] as f32).map(|j| j as u32).collect();
        if ids.is_empty() {
            out.push(Gray::new(1, 1, 0.0));
            continue;
        }
        let y0 = ids.iter().map(|&j| st[j as usize].y0).min().unwrap();
        let y1 = ids.iter().map(|&j| st[j as usize].y1).max().unwrap();
        let mut sel = vec![false; alpha.w * (y1 - y0)];
        for y in y0..y1 {
            for x in 0..alpha.w {
                sel[(y - y0) * alpha.w + x] = ids.contains(&lab[y * alpha.w + x]);
            }
        }
        let sel = img::dilate_mask(&sel, alpha.w, y1 - y0, 1);
        let mut g = alpha.crop(0, y0, alpha.w, y1);
        for (v, k) in g.d.iter_mut().zip(&sel) {
            if !k {
                *v = 0.0;
            }
        }
        out.push(g);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A found row: ink from x = 0 to `w`.
    fn row(w: usize) -> Gray {
        let mut g = Gray::new(400, 4, 0.0);
        for x in 0..w {
            g.d[x] = 1.0;
        }
        g
    }

    #[test]
    fn a_line_may_go_on_over_two_rows() {
        let text = ["kitob daftar ruchka qalam", "maktab bola"];
        // the first line wrapped after "daftar"
        let found = [row(120), row(115), row(105)];
        let (pairs, _) = match_lines(&found, &text);
        let got: Vec<(usize, usize, &str)> = pairs.iter().map(|p| (p.0, p.1, p.2.as_str())).collect();
        assert_eq!(got, vec![(0, 0, "kitob daftar"), (1, 0, "ruchka qalam"), (2, 1, "maktab bola")]);
        // written as typed: one row a line
        let (pairs, _) = match_lines(&[row(235), row(105)], &text);
        assert_eq!(pairs.iter().map(|p| (p.0, p.1)).collect::<Vec<_>>(), vec![(0, 0), (1, 1)]);
    }
}
