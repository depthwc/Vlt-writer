//! Display formulas laid out in two dimensions with handwritten glyphs: stacked fractions,
//! square roots with a drawn radical, super/subscripts. A LaTeX subset is parsed into a small
//! tree, then each node becomes a box (width, height above / below the baseline, elements).

use crate::ingest::text::TEX;
use crate::layout::{Composer, Elem};

#[derive(Debug, Clone, PartialEq)]
pub enum M {
    Text(String),
    Row(Vec<M>),
    Frac(Box<M>, Box<M>),
    Sqrt(Box<M>),
    Sup(Box<M>, Box<M>),
    Sub(Box<M>, Box<M>),
}

struct P<'a> {
    s: &'a [char],
    i: usize,
}

impl P<'_> {
    fn peek(&self) -> Option<char> {
        self.s.get(self.i).copied()
    }
    fn group(&mut self) -> M {
        // {...} or a single token
        match self.peek() {
            Some('{') => {
                self.i += 1;
                let r = self.row(Some('}'));
                self.i += 1;
                r
            }
            Some('\\') => self.command(),
            Some(c) => {
                self.i += 1;
                M::Text(c.to_string())
            }
            None => M::Text(String::new()),
        }
    }
    fn command(&mut self) -> M {
        self.i += 1;
        let start = self.i;
        while self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
            self.i += 1;
        }
        if self.i == start {
            // \, \; \% etc.
            let c = self.peek().unwrap_or(' ');
            self.i += 1;
            return M::Text(if c == ',' || c == ';' { " ".into() } else { c.to_string() });
        }
        let name: String = self.s[start..self.i].iter().collect();
        match name.as_str() {
            "frac" | "dfrac" | "tfrac" => {
                let a = self.group();
                let b = self.group();
                M::Frac(Box::new(a), Box::new(b))
            }
            "sqrt" => M::Sqrt(Box::new(self.group())),
            "left" | "right" | "displaystyle" => M::Text(String::new()),
            _ => {
                let key = format!("\\{name}");
                M::Text(TEX.iter().find(|(k, _)| *k == key).map(|(_, v)| v.to_string()).unwrap_or(name))
            }
        }
    }
    fn row(&mut self, until: Option<char>) -> M {
        let mut items: Vec<M> = vec![];
        let mut text = String::new();
        while let Some(c) = self.peek() {
            if Some(c) == until {
                break;
            }
            match c {
                '^' | '_' => {
                    self.i += 1;
                    let base = if !text.is_empty() {
                        let last = text.pop().unwrap();
                        if !text.is_empty() {
                            items.push(M::Text(std::mem::take(&mut text)));
                        }
                        M::Text(last.to_string())
                    } else {
                        items.pop().unwrap_or(M::Text(String::new()))
                    };
                    let arg = self.group();
                    items.push(if c == '^' { M::Sup(Box::new(base), Box::new(arg)) } else { M::Sub(Box::new(base), Box::new(arg)) });
                }
                '{' | '\\' => {
                    if !text.is_empty() {
                        items.push(M::Text(std::mem::take(&mut text)));
                    }
                    items.push(self.group());
                }
                '}' => {
                    self.i += 1;
                }
                _ => {
                    text.push(c);
                    self.i += 1;
                }
            }
        }
        if !text.is_empty() {
            items.push(M::Text(text));
        }
        if items.len() == 1 { items.pop().unwrap() } else { M::Row(items) }
    }
}

pub fn parse(tex: &str) -> M {
    let chars: Vec<char> = tex.chars().collect();
    P { s: &chars, i: 0 }.row(None)
}

/// A laid-out formula part: elements relative to its own baseline-left origin (mm).
pub struct MBox {
    pub elems: Vec<Elem>,
    pub w: f32,
    /// height above the baseline (positive)
    pub asc: f32,
    /// depth below the baseline (positive)
    pub desc: f32,
}

fn shift(e: Elem, dx: f32, dy: f32) -> Elem {
    match e {
        Elem::G { gid, x, l, dy: d, s, r, sh, seed, wi, ci, p } => Elem::G { gid, x: x + dx, l, dy: d + dy, s, r, sh, seed, wi, ci, p },
        Elem::S { pts, w, p } => Elem::S { pts: pts.into_iter().map(|[x, l, d]| [x + dx, l, d + dy]).collect(), w, p },
    }
}

fn stroke(pts: Vec<[f32; 2]>, c: &Composer) -> Elem {
    let w = vec![0.85 * c.pen_mm * c.p.size; pts.len()];
    Elem::S { pts: pts.into_iter().map(|[x, y]| [x, f32::NAN, y]).collect(), w, p: c.pressure }
}

pub fn layout(m: &M, c: &mut Composer, scale: f32, wi: &mut u32) -> MBox {
    let xh = c.xh * scale;
    match m {
        M::Text(t) => {
            let mut out = vec![];
            let mut x = 0.0;
            let words: Vec<&str> = t.split(' ').collect();
            for (k, w) in words.iter().enumerate() {
                if k > 0 {
                    x += 0.5 * xh;
                }
                if w.is_empty() {
                    continue;
                }
                let prev = c.size_mul;
                c.size_mul = scale;
                let (els, ww) = c.word(w, *wi);
                c.size_mul = prev;
                *wi += 1;
                out.extend(els.into_iter().map(|e| shift(e, x, 0.0)));
                x += ww;
            }
            MBox { elems: out, w: x, asc: 1.25 * xh, desc: 0.4 * xh }
        }
        M::Row(items) => {
            let mut out = vec![];
            let (mut x, mut asc, mut desc) = (0.0f32, 0.0f32, 0.0f32);
            for (k, it) in items.iter().enumerate() {
                let b = layout(it, c, scale, wi);
                if k > 0 && b.w > 0.0 {
                    x += 0.18 * xh;
                }
                out.extend(b.elems.into_iter().map(|e| shift(e, x, 0.0)));
                x += b.w;
                asc = asc.max(b.asc);
                desc = desc.max(b.desc);
            }
            MBox { elems: out, w: x, asc, desc }
        }
        M::Sup(base, up) | M::Sub(base, up) => {
            let is_sup = matches!(m, M::Sup(..));
            let b = layout(base, c, scale, wi);
            // written by hand, exponents are only a little smaller (smaller, inside a fraction they were specks)
            let u = layout(up, c, scale * 0.72, wi);
            let dy = if is_sup { -0.78 * xh } else { 0.38 * xh };
            let x = b.w + 0.06 * xh;
            let mut elems = b.elems;
            elems.extend(u.elems.into_iter().map(|e| shift(e, x, dy)));
            MBox {
                elems,
                w: x + u.w,
                asc: if is_sup { b.asc.max(-dy + u.asc) } else { b.asc },
                desc: if is_sup { b.desc } else { b.desc.max(dy + u.desc) },
            }
        }
        M::Frac(num, den) => {
            let n = layout(num, c, scale * 0.92, wi);
            let d = layout(den, c, scale * 0.92, wi);
            let w = n.w.max(d.w) + 0.6 * xh;
            let axis = -0.5 * xh;
            let gap = 0.28 * xh;
            let nb = axis - gap - n.desc;
            let db = axis + gap + d.asc;
            let mut elems: Vec<Elem> = n.elems.into_iter().map(|e| shift(e, (w - n.w) / 2.0, nb)).collect();
            elems.extend(d.elems.into_iter().map(|e| shift(e, (w - d.w) / 2.0, db)));
            let wob = c.n(0.04 * xh);
            elems.push(stroke((0..8).map(|k| {
                let t = k as f32 / 7.0;
                [-0.1 * xh + t * (w + 0.2 * xh), axis + wob * (t * 3.0).sin()]
            }).collect(), c));
            MBox { elems, w, asc: -nb + n.asc, desc: db + d.desc }
        }
        M::Sqrt(inner) => {
            let b = layout(inner, c, scale, wi);
            let top = -(b.asc + 0.18 * xh);
            let x0 = 0.95 * xh;
            let mut elems: Vec<Elem> = b.elems.into_iter().map(|e| shift(e, x0, 0.0)).collect();
            elems.push(stroke(vec![
                [0.0, -0.4 * xh],
                [0.18 * xh, -0.5 * xh],
                [0.45 * xh, 0.35 * b.desc.max(0.15 * xh)],
                [0.8 * xh, top],
                [x0 + b.w + 0.25 * xh, top + c.n(0.03 * xh)],
            ], c));
            MBox { elems, w: x0 + b.w + 0.3 * xh, asc: -top + 0.1 * xh, desc: b.desc.max(0.35 * xh) }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_latex() {
        let m = parse(r"x^2 + \frac{a+b}{\sqrt{c}} = \pi");
        let M::Row(items) = &m else { panic!("{m:?}") };
        assert!(matches!(items[0], M::Sup(..)));
        assert!(items.iter().any(|i| matches!(i, M::Frac(_, d) if matches!(**d, M::Sqrt(_)))));
        assert!(items.iter().any(|i| *i == M::Text(" = π".into()) || matches!(i, M::Text(t) if t.contains('π'))));
    }
}
