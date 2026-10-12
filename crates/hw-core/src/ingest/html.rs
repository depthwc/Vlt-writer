//! HTML (also what the clipboard gives when pasting from Word / Google Docs).

use scraper::{ElementRef, Html, Node};

use super::{Block, END, SUB, SUP};
use crate::charset;

fn inline(e: ElementRef) -> String {
    let mut s = String::new();
    for c in e.children() {
        match c.value() {
            Node::Text(t) => s.push_str(t),
            Node::Element(el) => {
                if let Some(ce) = ElementRef::wrap(c) {
                    match el.name() {
                        "sup" => s.push_str(&format!("{SUP}{}{END}", inline(ce))),
                        "sub" => s.push_str(&format!("{SUB}{}{END}", inline(ce))),
                        "b" | "strong" => s.push_str(&format!("{}{}{}", super::BOLD, inline(ce), super::BOLD_END)),
                        "u" => s.push_str(&format!("{}{}{}", super::UNDER, inline(ce), super::UNDER_END)),
                        "br" => s.push(' '),
                        "script" | "style" => {}
                        _ => s.push_str(&inline(ce)),
                    }
                }
            }
            _ => {}
        }
    }
    s
}

fn clean(s: &str) -> String {
    charset::normalize(&s.split_whitespace().collect::<Vec<_>>().join(" "))
}

fn walk(e: ElementRef, out: &mut Vec<Block>) {
    for c in e.children() {
        let Some(ce) = ElementRef::wrap(c) else {
            if let Node::Text(t) = c.value() {
                let t = clean(t);
                if !t.is_empty() {
                    out.push(Block::P { text: t });
                }
            }
            continue;
        };
        let name = ce.value().name();
        match name {
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                let t = clean(&inline(ce));
                if !t.is_empty() {
                    out.push(Block::H { text: t, level: name[1..].parse().unwrap_or(1) });
                }
            }
            "p" => {
                let t = clean(&inline(ce));
                if !t.is_empty() {
                    out.push(Block::P { text: t });
                }
            }
            "ul" | "ol" => {
                let mut i = 0;
                for li in ce.children().filter_map(ElementRef::wrap).filter(|x| x.value().name() == "li") {
                    i += 1;
                    let t = clean(&inline(li));
                    if !t.is_empty() {
                        out.push(Block::Li { text: t, marker: if name == "ol" { format!("{i}.") } else { "-".into() } });
                    }
                }
            }
            "table" => {
                let sel = scraper::Selector::parse("tr").unwrap();
                let cell = scraper::Selector::parse("td, th").unwrap();
                let rows: Vec<Vec<String>> = ce.select(&sel).map(|tr| tr.select(&cell).map(|td| clean(&inline(td))).collect()).collect();
                if !rows.is_empty() {
                    out.push(Block::Table { rows });
                }
            }
            "hr" => out.push(Block::Rule),
            "script" | "style" | "head" | "title" | "meta" => {}
            _ => walk(ce, out),
        }
    }
}

pub fn from_html(src: &str) -> Vec<Block> {
    let doc = Html::parse_document(src);
    let mut out = vec![];
    walk(doc.root_element(), &mut out);
    out
}
