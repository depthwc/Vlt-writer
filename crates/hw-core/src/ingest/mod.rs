//! Read documents into one block list the layout engine understands.
//! Inside text, SUP..END marks superscript and SUB..END subscript.

use std::path::Path;

use serde::{Deserialize, Serialize};

pub mod docx;
pub mod html;
pub mod odt;
pub mod pdf;
pub mod rtf;
pub mod text;
pub mod xml;

pub const SUP: char = '\u{1}';
pub const SUB: char = '\u{2}';
pub const END: char = '\u{3}';
/// bold (written with more pressure) and underline (drawn line) spans
pub const BOLD: char = '\u{4}';
pub const BOLD_END: char = '\u{5}';
/// A new line inside a paragraph (Shift+Enter in the editor).
pub const LINE_BREAK: char = '\u{2028}';
/// In the editor's text: an empty paragraph, kept as an empty line.
pub const BLANK_LINE: char = '\u{e}';
pub const UNDER: char = '\u{6}';
pub const UNDER_END: char = '\u{7}';

pub fn is_marker(c: char) -> bool {
    ('\u{1}'..='\u{7}').contains(&c)
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "t", rename_all = "lowercase")]
pub enum Block {
    H { text: String, level: u8 },
    P { text: String },
    Li { text: String, marker: String },
    Table { rows: Vec<Vec<String>> },
    Math { tex: String },
    Rule,
    Br,
}

pub fn load(path: &Path, pdfium: Option<&Path>) -> anyhow::Result<Vec<Block>> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let data = std::fs::read(path)?;
    match ext.as_str() {
        "txt" | "md" | "markdown" | "" => Ok(text::from_text(&crate::io::decode_text(&data))),
        "docx" => docx::from_docx(&data),
        "odt" => odt::from_odt(&data),
        "html" | "htm" => Ok(html::from_html(&String::from_utf8_lossy(&data))),
        "rtf" => rtf::from_rtf(&String::from_utf8_lossy(&data)),
        "pdf" => pdf::from_pdf(&data, pdfium),
        other => anyhow::bail!("unsupported file type: .{other}"),
    }
}

/// Plain markdown-ish text of blocks (for showing / editing as text).
pub fn to_text(blocks: &[Block]) -> String {
    let mut out = vec![];
    for b in blocks {
        match b {
            Block::H { text, level } => out.push(format!("{} {}", "#".repeat(*level as usize), text)),
            Block::P { text } => out.push(if text.is_empty() { BLANK_LINE.to_string() } else { text.clone() }),
            Block::Li { text, marker } => out.push(format!("{marker} {text}")),
            Block::Table { rows } => out.extend(rows.iter().map(|r| format!("| {} |", r.join(" | ")))),
            Block::Math { tex } => out.push(format!("$${tex}$$")),
            Block::Rule => out.push("---".into()),
            Block::Br => out.push("\u{c}".into()),
        }
        out.push(String::new());
    }
    out.join("\n")
        // a page break (form feed) at either end stays
        .trim_matches(|c: char| c.is_whitespace() && c != '\u{c}')
        .replace(SUP, "$^{")
        .replace(SUB, "$_{")
        .replace(END, "}$")
        .replace([BOLD, BOLD_END], "**")
        .replace(UNDER, "<u>")
        .replace(UNDER_END, "</u>")
}
