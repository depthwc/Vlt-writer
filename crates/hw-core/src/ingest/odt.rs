//! ODT (OpenDocument text): headings, paragraphs, lists, tables, soft page breaks.

use super::xml::{self, El, Node};
use super::Block;
use crate::charset;

fn text_of(e: &El) -> String {
    let mut s = String::new();
    for k in &e.kids {
        match k {
            Node::Text(t) => s.push_str(t),
            Node::El(c) => match c.name.as_str() {
                "text:s" => s.push_str(&" ".repeat(c.attr("text:c").and_then(|v| v.parse().ok()).unwrap_or(1))),
                "text:tab" | "text:line-break" => s.push(' '),
                "text:note" => {}
                _ => s.push_str(&text_of(c)),
            },
        }
    }
    s
}

fn walk(e: &El, out: &mut Vec<Block>) {
    for c in e.els() {
        match c.name.as_str() {
            "text:h" => {
                let level = c.attr("text:outline-level").and_then(|v| v.parse().ok()).unwrap_or(1u8);
                let t = charset::normalize(&text_of(c)).trim().to_string();
                if !t.is_empty() {
                    out.push(Block::H { text: t, level });
                }
            }
            "text:p" => {
                let t = charset::normalize(&text_of(c)).trim().to_string();
                if !t.is_empty() {
                    out.push(Block::P { text: t });
                }
            }
            "text:list" => {
                let numbered = c.attr("text:style-name").is_some_and(|s| s.to_lowercase().contains("num"));
                let mut i = 0;
                for item in c.els().filter(|x| x.name == "text:list-item") {
                    for p in item.els().filter(|x| x.name == "text:p" || x.name == "text:h") {
                        i += 1;
                        let t = charset::normalize(&text_of(p)).trim().to_string();
                        if !t.is_empty() {
                            out.push(Block::Li { text: t, marker: if numbered { format!("{i}.") } else { "-".into() } });
                        }
                    }
                    for sub in item.els().filter(|x| x.name == "text:list") {
                        let mut tmp = El { name: "x".into(), ..Default::default() };
                        tmp.kids.push(Node::El(sub.clone()));
                        walk(&tmp, out);
                    }
                }
            }
            "table:table" => {
                let mut rows_el = vec![];
                c.find_all("table:table-row", &mut rows_el);
                let rows: Vec<Vec<String>> = rows_el
                    .iter()
                    .map(|r| r.els().filter(|x| x.name == "table:table-cell").map(|cell| charset::normalize(&text_of(cell)).trim().to_string()).collect())
                    .collect();
                if !rows.is_empty() {
                    out.push(Block::Table { rows });
                }
            }
            "text:soft-page-break" => out.push(Block::Br),
            "text:section" => walk(c, out),
            _ => {}
        }
    }
}

pub fn from_odt(data: &[u8]) -> anyhow::Result<Vec<Block>> {
    let doc = xml::parse(&xml::zip_entry(data, "content.xml").ok_or_else(|| anyhow::anyhow!("not an .odt file"))?)?;
    let body = doc.path(&["office:document-content", "office:body", "office:text"]).ok_or_else(|| anyhow::anyhow!("no text body"))?;
    let mut out = vec![];
    walk(body, &mut out);
    Ok(out)
}
