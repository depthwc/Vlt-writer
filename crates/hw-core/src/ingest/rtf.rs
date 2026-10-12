//! RTF: the text, paragraph by paragraph (fonts, colours, pictures and the like are skipped).
//! Read here rather than with a library: the one used before ran paragraphs together.

use super::{text, Block};

pub fn from_rtf(src: &str) -> anyhow::Result<Vec<Block>> {
    anyhow::ensure!(src.trim_start().starts_with("{\\rtf"), "not an RTF document");
    // blank line between paragraphs so the text reader keeps them apart
    Ok(text::from_text(&rtf_text(src).split('\n').map(str::trim_end).collect::<Vec<_>>().join("\n\n")))
}

/// Groups whose text is not the document's.
const SKIP: &[&str] = &[
    "fonttbl", "colortbl", "stylesheet", "info", "pict", "header", "headerl", "headerr", "headerf", "footer", "footerl",
    "footerr", "footerf", "footnote", "object", "listtable", "listoverridetable", "rsidtbl", "generator", "xmlnstbl",
    "themedata", "colorschememapping", "datastore", "latentstyles", "fldinst", "bkmkstart", "bkmkend", "filetbl",
    "revtbl", "pgdsctbl", "shppict", "nonshppict", "template",
];

/// One byte of 8-bit text in the document's code page (Cyrillic 1251, else Latin).
fn byte_char(v: u8, cp: i32) -> char {
    match (v, cp) {
        (0..=0x7F, _) => v as char,
        (_, 1251) => crate::io::decode_text(&[v]).chars().next().unwrap_or('?'),
        _ => v as char,
    }
}

/// The document's text, one paragraph per line.
fn rtf_text(src: &str) -> String {
    let b = src.as_bytes();
    let mut out = String::new();
    // per group: skipped, and how many characters follow a \u as its stand-in
    let mut stack: Vec<(bool, usize)> = vec![(false, 1)];
    let mut stand_ins = 0usize;
    let mut cp = 1252;
    let mut i = 0;
    let push = |out: &mut String, stack: &[(bool, usize)], stand_ins: &mut usize, c: char| {
        if *stand_ins > 0 {
            *stand_ins -= 1;
        } else if !stack.last().is_some_and(|g| g.0) {
            out.push(c);
        }
    };
    while i < b.len() {
        match b[i] {
            b'{' => {
                let top = *stack.last().unwrap_or(&(false, 1));
                stack.push(top);
                i += 1;
            }
            b'}' => {
                stack.pop();
                if stack.is_empty() {
                    break;
                }
                i += 1;
            }
            b'\r' | b'\n' => i += 1,
            b'\\' => {
                i += 1;
                let Some(&n) = b.get(i) else { break };
                if n == b'\'' {
                    // a byte in hex
                    let v = src.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok()).unwrap_or(b'?');
                    i += 3;
                    push(&mut out, &stack, &mut stand_ins, byte_char(v, cp));
                    continue;
                }
                if !n.is_ascii_alphabetic() {
                    // \* : an optional destination the reader does not know
                    match n {
                        b'*' => {
                            if let Some(g) = stack.last_mut() {
                                g.0 = true;
                            }
                        }
                        b'\\' | b'{' | b'}' => push(&mut out, &stack, &mut stand_ins, n as char),
                        b'~' => push(&mut out, &stack, &mut stand_ins, ' '),
                        _ => {}
                    }
                    i += 1;
                    continue;
                }
                let start = i;
                while i < b.len() && b[i].is_ascii_alphabetic() {
                    i += 1;
                }
                let word = &src[start..i];
                let ns = i;
                if i < b.len() && (b[i] == b'-' || b[i].is_ascii_digit()) {
                    i += 1;
                    while i < b.len() && b[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                let num: Option<i32> = src[ns..i].parse().ok();
                if b.get(i) == Some(&b' ') {
                    i += 1;
                }
                let skipped = stack.last().is_some_and(|g| g.0);
                match word {
                    "par" | "line" | "sect" | "page" | "row" if !skipped => out.push('\n'),
                    "tab" | "cell" if !skipped => out.push(' '),
                    "u" => {
                        let v = num.unwrap_or(0);
                        let v = if v < 0 { v + 65536 } else { v } as u32;
                        if let Some(c) = char::from_u32(v).filter(|_| !skipped) {
                            out.push(c);
                        }
                        stand_ins = stack.last().map_or(1, |g| g.1);
                    }
                    "uc" => {
                        if let Some(g) = stack.last_mut() {
                            g.1 = num.unwrap_or(1).max(0) as usize;
                        }
                    }
                    "ansicpg" => cp = num.unwrap_or(1252),
                    w if SKIP.contains(&w) => {
                        if let Some(g) = stack.last_mut() {
                            g.0 = true;
                        }
                    }
                    _ => {}
                }
            }
            c => {
                push(&mut out, &stack, &mut stand_ins, byte_char(c, cp));
                i += 1;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paragraphs_stay_apart() {
        let src = r"{\rtf1\ansi\ansicpg1251\deff0{\fonttbl{\f0 Arial;}}{\*\generator Word;}\f0\fs24 {\b Uy vazifasi}\par\par Bugun o\u699?qidik\par \'cf\'f0\'e8\'e2\'e5\'f2\par}";
        let b = from_rtf(src).unwrap();
        assert_eq!(b, vec![
            Block::P { text: "Uy vazifasi".into() },
            Block::P { text: "Bugun oʻqidik".into() },
            Block::P { text: "Привет".into() },
        ]);
    }
}
