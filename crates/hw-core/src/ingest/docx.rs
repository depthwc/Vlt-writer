//! DOCX: body order kept; headings, lists (bullet vs numbered from numbering.xml), tables,
//! bottom borders as drawn lines, page breaks, super/subscript runs and Office Math (OMML).

use std::collections::HashMap;

use super::xml::{self, El, Node};
use super::{Block, END, SUB, SUP};
use crate::charset;

fn para_text(p: &El) -> (String, bool) {
    let mut out = String::new();
    let mut page_break = false;
    walk_runs(p, &mut out, &mut page_break);
    (out, page_break)
}

fn walk_runs(e: &El, out: &mut String, pb: &mut bool) {
    for k in e.els() {
        match k.name.as_str() {
            "w:r" => {
                let mode = k.path(&["w:rPr", "w:vertAlign"]).and_then(|v| v.attr("w:val")).unwrap_or("");
                let mut t = String::new();
                for c in k.els() {
                    match c.name.as_str() {
                        "w:t" => t.push_str(&c.text()),
                        "w:tab" => t.push(' '),
                        "w:br" if c.attr("w:type") == Some("page") => *pb = true,
                        _ => {}
                    }
                }
                let rpr = k.child("w:rPr");
                let on = |name: &str| rpr.and_then(|r| r.child(name)).is_some_and(|b| !matches!(b.attr("w:val"), Some("0" | "false" | "none")));
                let mut t = match mode {
                    "superscript" if !t.is_empty() => format!("{SUP}{t}{END}"),
                    "subscript" if !t.is_empty() => format!("{SUB}{t}{END}"),
                    _ => t,
                };
                if !t.trim().is_empty() {
                    if on("w:b") {
                        t = format!("{}{t}{}", super::BOLD, super::BOLD_END);
                    }
                    if on("w:u") {
                        t = format!("{}{t}{}", super::UNDER, super::UNDER_END);
                    }
                }
                out.push_str(&t);
            }
            "m:oMath" => out.push_str(&omml(k)),
            "m:oMathPara" => {
                for m in k.els().filter(|m| m.name == "m:oMath") {
                    out.push_str(&omml(m));
                }
            }
            "w:hyperlink" | "w:smartTag" | "w:ins" | "w:sdt" | "w:sdtContent" | "w:fldSimple" => walk_runs(k, out, pb),
            _ => {}
        }
    }
}

/// Office Math to linear text: fractions (a)/(b), sup/sub markers, √(x).
pub fn omml(e: &El) -> String {
    let part = |name: &str| e.child(name).map(|c| c.els().map(omml).collect::<String>()).unwrap_or_default();
    match e.name.trim_start_matches("m:") {
        "t" => e.text(),
        "f" => format!("({})/({})", part("m:num"), part("m:den")),
        "sSup" => format!("{}{SUP}{}{END}", part("m:e"), part("m:sup")),
        "sSub" => format!("{}{SUB}{}{END}", part("m:e"), part("m:sub")),
        "sSubSup" => format!("{}{SUB}{}{END}{SUP}{}{END}", part("m:e"), part("m:sub"), part("m:sup")),
        "rad" => format!("√({})", part("m:e")),
        "nary" => {
            let op = e.path(&["m:naryPr", "m:chr"]).and_then(|c| c.attr("m:val")).unwrap_or("∫").to_string();
            format!("{op}{}", part("m:e"))
        }
        "rPr" | "ctrlPr" | "fPr" | "radPr" | "sSupPr" | "sSubPr" | "naryPr" | "deg" => String::new(),
        _ => e.kids.iter().map(|k| match k {
            Node::El(c) => omml(c),
            Node::Text(_) => String::new(),
        }).collect(),
    }
}

fn numbered(numbering: &Option<El>, num_id: &str) -> bool {
    let Some(n) = numbering else { return false };
    let mut nums = vec![];
    n.find_all("w:num", &mut nums);
    let Some(aid) = nums.iter().find(|x| x.attr("w:numId") == Some(num_id)).and_then(|x| x.child("w:abstractNumId")).and_then(|a| a.attr("w:val")) else {
        return false;
    };
    let mut abs = vec![];
    n.find_all("w:abstractNum", &mut abs);
    abs.iter()
        .find(|a| a.attr("w:abstractNumId") == Some(aid))
        .and_then(|a| a.path(&["w:lvl", "w:numFmt"]))
        .and_then(|f| f.attr("w:val"))
        .is_some_and(|v| v != "bullet" && v != "none")
}

pub fn from_docx(data: &[u8]) -> anyhow::Result<Vec<Block>> {
    let doc = xml::parse(&xml::zip_entry(data, "word/document.xml").ok_or_else(|| anyhow::anyhow!("not a .docx file"))?)?;
    let styles: HashMap<String, String> = xml::zip_entry(data, "word/styles.xml")
        .and_then(|s| xml::parse(&s).ok())
        .map(|st| {
            let mut v = vec![];
            st.find_all("w:style", &mut v);
            v.iter()
                .filter_map(|s| Some((s.attr("w:styleId")?.to_string(), s.child("w:name")?.attr("w:val")?.to_string())))
                .collect()
        })
        .unwrap_or_default();
    let numbering = xml::zip_entry(data, "word/numbering.xml").and_then(|s| xml::parse(&s).ok());
    let body = doc.path(&["w:document", "w:body"]).ok_or_else(|| anyhow::anyhow!("no body"))?;
    let mut blocks = vec![];
    let mut counters: HashMap<String, usize> = HashMap::new();
    for child in body.els() {
        match child.name.as_str() {
            "w:p" => {
                let (raw, pb) = para_text(child);
                let text = charset::normalize(&raw).trim().to_string();
                let ppr = child.child("w:pPr");
                let sid = ppr.and_then(|p| p.child("w:pStyle")).and_then(|s| s.attr("w:val")).unwrap_or("");
                let sname = styles.get(sid).cloned().unwrap_or_else(|| sid.to_string());
                let num = ppr.and_then(|p| p.child("w:numPr"));
                let border = ppr.and_then(|p| p.path(&["w:pBdr", "w:bottom"])).is_some();
                if !text.is_empty() {
                    let lower = sname.to_lowercase();
                    if lower.starts_with("heading") || lower == "title" {
                        let lvl: u8 = sname.chars().filter(|c| c.is_ascii_digit()).collect::<String>().parse().unwrap_or(1);
                        blocks.push(Block::H { text, level: lvl.max(1) });
                    } else if num.is_some() || sname.contains("List") {
                        let nid = num.and_then(|n| n.child("w:numId")).and_then(|n| n.attr("w:val")).unwrap_or(&sname).to_string();
                        let marker = if sname.contains("Number") || numbered(&numbering, &nid) {
                            let c = counters.entry(nid).or_insert(0);
                            *c += 1;
                            format!("{c}.")
                        } else {
                            "-".to_string()
                        };
                        blocks.push(Block::Li { text, marker });
                    } else {
                        blocks.push(Block::P { text });
                    }
                }
                if border {
                    blocks.push(Block::Rule);
                }
                if pb {
                    blocks.push(Block::Br);
                }
            }
            "w:tbl" => {
                let mut rows = vec![];
                for tr in child.els().filter(|e| e.name == "w:tr") {
                    let row: Vec<String> = tr
                        .els()
                        .filter(|e| e.name == "w:tc")
                        .map(|tc| {
                            let mut ps = vec![];
                            tc.find_all("w:p", &mut ps);
                            charset::normalize(&ps.iter().map(|p| para_text(p).0).collect::<Vec<_>>().join(" ")).trim().to_string()
                        })
                        .collect();
                    rows.push(row);
                }
                if !rows.is_empty() {
                    blocks.push(Block::Table { rows });
                }
            }
            _ => {}
        }
    }
    Ok(blocks)
}
