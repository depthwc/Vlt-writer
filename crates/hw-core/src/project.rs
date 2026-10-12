//! A project: the document, chosen hand and style, pen, per-paper placement, exports.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::camera::ShotStyle;
use crate::ingest::Block;
use crate::layout::Params;
use crate::paper::Placement;
use crate::pen::Pen;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub created: f64,
    pub updated: f64,
    pub hand: Option<String>,
    /// editor document (Tiptap JSON), kept so the editor reopens exactly
    #[serde(default)]
    pub doc: serde_json::Value,
    /// markdown-lite text from the editor; blocks are parsed from it on save
    #[serde(default)]
    pub source: String,
    pub blocks: Vec<Block>,
    #[serde(default)]
    pub params: Params,
    #[serde(default)]
    pub pen: Pen,
    #[serde(default)]
    pub shot: ShotStyle,
    pub paper: Option<String>,
    #[serde(default)]
    pub place: HashMap<String, Placement>,
    #[serde(default)]
    pub outputs: Vec<String>,
}

impl Project {
    /// The blocks to write, from the editor's text. A document from the editor is always
    /// what the editor holds, empty too (an emptied page writes nothing); one that never was
    /// in the editor keeps its blocks.
    pub fn sync_blocks(&mut self) {
        let from_editor = self.doc.get("type").and_then(|t| t.as_str()) == Some("doc");
        if from_editor || !self.source.is_empty() {
            self.blocks = crate::ingest::text::from_text(&self.source);
        }
    }

    pub fn new(id: &str, name: &str) -> Self {
        let t = crate::hand::now();
        Project {
            id: id.into(),
            name: name.into(),
            created: t,
            updated: t,
            hand: None,
            doc: serde_json::Value::Null,
            source: String::new(),
            blocks: vec![],
            params: Params::default(),
            pen: Pen::default(),
            shot: ShotStyle::default(),
            paper: None,
            place: HashMap::new(),
            outputs: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_emptied_page_writes_nothing() {
        let mut p = Project::new("p", "p");
        p.doc = serde_json::json!({"type": "doc", "content": [{"type": "paragraph", "content": [{"type": "text", "text": "salom"}]}]});
        p.source = "salom".into();
        p.sync_blocks();
        assert_eq!(p.blocks.len(), 1);
        // everything deleted in the editor: nothing is left to write
        p.doc = serde_json::json!({"type": "doc", "content": [{"type": "paragraph"}]});
        p.source = String::new();
        p.sync_blocks();
        assert!(p.blocks.is_empty(), "{:?}", p.blocks);
        // a document that never was in the editor keeps its blocks
        let mut q = Project::new("q", "q");
        q.blocks = crate::ingest::text::from_text("eski matn");
        q.sync_blocks();
        assert_eq!(q.blocks.len(), 1);
    }
}
