//! Plain text / markdown-lite: # headings, - and 1. lists, | tables |, ---, $math$, $$display$$.

use std::sync::LazyLock;

use regex::Regex;

use super::{Block, BLANK_LINE, END, SUB, SUP};
use crate::charset;

pub const TEX: &[(&str, &str)] = &[
    (r"\times", "×"), (r"\cdot", "·"), (r"\div", "÷"), (r"\pm", "±"), (r"\leq", "≤"), (r"\le", "≤"),
    (r"\geq", "≥"), (r"\ge", "≥"), (r"\neq", "≠"), (r"\ne", "≠"), (r"\approx", "≈"), (r"\infty", "∞"),
    (r"\pi", "π"), (r"\sum", "∑"), (r"\int", "∫"), (r"\alpha", "α"), (r"\beta", "β"), (r"\gamma", "γ"),
    (r"\Delta", "Δ"), (r"\delta", "δ"), (r"\theta", "θ"), (r"\lambda", "λ"), (r"\mu", "μ"),
    (r"\sigma", "σ"), (r"\varphi", "φ"), (r"\phi", "φ"), (r"\omega", "ω"), (r"\rightarrow", "→"),
    (r"\to", "→"), (r"\degree", "°"), (r"\circ", "°"), (r"\%", "%"), (r"\left", ""), (r"\right", ""),
    (r"\,", " "), (r"\;", " "), (r"\quad", "  "),
];

static FRAC: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\\frac\s*\{([^{}]*)\}\s*\{([^{}]*)\}").unwrap());
static SQRT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\\sqrt\s*\{([^{}]*)\}").unwrap());
static SUPB: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\^\{([^{}]*)\}").unwrap());
static SUBB: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"_\{([^{}]*)\}").unwrap());
static SUP1: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\^(\S)").unwrap());
static SUB1: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"_(\S)").unwrap());
static INLINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\$([^$]+)\$").unwrap());
static BOLD_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\*\*|__)(.+?)(\*\*|__)").unwrap());
static UNDER_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<u>(.+?)</u>").unwrap());
static ITALIC_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(^|[\s(])[*_]([^*_\s][^*_]*?)[*_]([\s).,!?;:]|$)").unwrap());
static LIST: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*(?:([-*•–])|(\d+)[.)])\s+(.*)$").unwrap());
static HEAD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(#{1,6})\s+(.*)$").unwrap());
static RULE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(-{3,}|\*{3,}|_{3,})$").unwrap());
static SEP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^:?-{2,}:?$").unwrap());

/// Inline LaTeX subset to linear text with sup/sub markers (used for $...$ inside a line).
pub fn latex_to_text(s: &str) -> String {
    let mut s = FRAC.replace_all(s, "($1)/($2)").into_owned();
    s = SQRT.replace_all(&s, "√($1)").into_owned();
    for (k, v) in TEX {
        s = s.replace(k, v);
    }
    s = SUPB.replace_all(&s, format!("{SUP}$1{END}").as_str()).into_owned();
    s = SUBB.replace_all(&s, format!("{SUB}$1{END}").as_str()).into_owned();
    s = SUP1.replace_all(&s, format!("{SUP}$1{END}").as_str()).into_owned();
    s = SUB1.replace_all(&s, format!("{SUB}$1{END}").as_str()).into_owned();
    s.replace(['{', '}', '\\'], "")
}

pub fn inline(t: &str) -> String {
    inline_raw(t).trim().to_string()
}

/// A paragraph's text: spaces at its start are kept (they indent it).
fn inline_para(t: &str) -> String {
    inline_raw(t).trim_end().to_string()
}

fn inline_raw(t: &str) -> String {
    let t = INLINE.replace_all(t, |c: &regex::Captures| latex_to_text(&c[1])).into_owned();
    let t = BOLD_RE.replace_all(&t, format!("{}$2{}", super::BOLD, super::BOLD_END).as_str()).into_owned();
    let t = UNDER_RE.replace_all(&t, format!("{}$1{}", super::UNDER, super::UNDER_END).as_str()).into_owned();
    let t = ITALIC_RE.replace_all(&t, "$1$2$3").into_owned();
    charset::normalize(&t)
}

pub fn from_text(src: &str) -> Vec<Block> {
    let mut blocks = vec![];
    let mut para: Vec<String> = vec![];
    let flush = |para: &mut Vec<String>, blocks: &mut Vec<Block>| {
        if !para.is_empty() {
            blocks.push(Block::P { text: inline_para(&para.join(" ")) });
            para.clear();
        }
    };
    let lines: Vec<&str> = src.split('\n').map(|l| l.trim_end_matches('\r')).collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let s = line.trim();
        // a page break: before the empty-line test, as trim() takes the form feed for a space
        if line.trim_matches([' ', '\t']) == "\u{c}" {
            flush(&mut para, &mut blocks);
            blocks.push(Block::Br);
        } else if s.is_empty() {
            flush(&mut para, &mut blocks);
        } else if s == BLANK_LINE.to_string() {
            // an empty line the person left
            flush(&mut para, &mut blocks);
            blocks.push(Block::P { text: String::new() });
        } else if RULE.is_match(s) {
            flush(&mut para, &mut blocks);
            blocks.push(Block::Rule);
        } else if s.starts_with("$$") {
            flush(&mut para, &mut blocks);
            let mut tex = s.trim_start_matches("$$").to_string();
            while !tex.trim_end().ends_with("$$") && i + 1 < lines.len() {
                i += 1;
                tex.push(' ');
                tex.push_str(lines[i].trim());
            }
            blocks.push(Block::Math { tex: tex.trim().trim_end_matches("$$").trim().to_string() });
        } else if s.starts_with('|') {
            flush(&mut para, &mut blocks);
            let mut rows = vec![];
            while i < lines.len() && lines[i].trim().starts_with('|') {
                let cells: Vec<String> = lines[i].trim().trim_matches('|').split('|').map(|c| c.trim().to_string()).collect();
                if !cells.iter().filter(|c| !c.is_empty()).all(|c| SEP.is_match(c)) {
                    rows.push(cells.iter().map(|c| inline(c)).collect());
                }
                i += 1;
            }
            blocks.push(Block::Table { rows });
            continue;
        } else if let Some(m) = HEAD.captures(s) {
            flush(&mut para, &mut blocks);
            blocks.push(Block::H { text: inline(&m[2]), level: m[1].len() as u8 });
        } else if let Some(m) = LIST.captures(line) {
            flush(&mut para, &mut blocks);
            let marker = if m.get(1).is_some() { "-".to_string() } else { format!("{}.", &m[2]) };
            blocks.push(Block::Li { text: inline(&m[3]), marker });
        } else {
            para.push(if para.is_empty() { line.trim_end() } else { s }.to_string());
        }
        i += 1;
    }
    flush(&mut para, &mut blocks);
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spaces_and_empty_lines_are_kept() {
        let b = from_text("Bir\n\n\u{e}\n\n   ikki  uch\u{2028}tort\u{a0}besh  ");
        assert_eq!(b, vec![
            Block::P { text: "Bir".into() },
            Block::P { text: String::new() },
            Block::P { text: "   ikki  uch\u{2028}tort besh".into() },
        ]);
    }

    #[test]
    fn a_page_break_from_the_editor_is_kept() {
        // the editor writes a page break as a form feed between empty lines
        let b = from_text("Bir\n\n\u{c}\n\nIkki");
        assert_eq!(b, vec![Block::P { text: "Bir".into() }, Block::Br, Block::P { text: "Ikki".into() }]);
        assert_eq!(from_text(&crate::ingest::to_text(&b)), b);
    }

    #[test]
    fn markdown_blocks() {
        let md = "# Uy vazifasi\n\nBugun biz o'qidik. Formula: $x^2 + y_1 = 5$\n\n- birinchi\n- ikkinchi\n\n| Ism | Baho |\n|---|---|\n| Ali | 5 |\n| Олег | 4 |\n\n---\n$$\\frac{a}{b}$$\nOxiri.";
        let b = from_text(md);
        let kinds: Vec<&str> = b.iter().map(|x| match x {
            Block::H { .. } => "h", Block::P { .. } => "p", Block::Li { .. } => "li", Block::Table { .. } => "table",
            Block::Rule => "rule", Block::Math { .. } => "math", Block::Br => "br",
        }).collect();
        assert_eq!(kinds, ["h", "p", "li", "li", "table", "rule", "math", "p"]);
        let Block::P { text } = &b[1] else { panic!() };
        assert!(text.contains("oʻqidik") && text.contains(&format!("{SUP}2{END}")));
        let Block::Table { rows } = &b[4] else { panic!() };
        assert_eq!(rows, &vec![vec!["Ism".to_string(), "Baho".into()], vec!["Ali".into(), "5".into()], vec!["Олег".into(), "4".into()]]);
    }
}
