//! What a document needs from a hand. Every character on a page is the person's own writing:
//! one they never wrote is reported, never made up. Versions made up by earlier versions of
//! the app (src = Auto) are removed from a hand when it is next opened or saved.

use std::collections::{HashMap, HashSet};

use crate::charset;
use crate::hand::{Hand, Src};
use crate::ingest::Block;

/// All the text of a document, as written (table cells and maths too).
fn text_of(blocks: &[Block]) -> String {
    let mut s = String::new();
    for b in blocks {
        match b {
            Block::P { text } | Block::H { text, .. } => s.push_str(text),
            Block::Li { text, marker } => {
                s.push_str(marker);
                s.push_str(text);
            }
            Block::Table { rows } => rows.iter().flatten().for_each(|c| s.push_str(c)),
            Block::Math { tex } => s.push_str(&crate::ingest::text::latex_to_text(tex)),
            Block::Rule | Block::Br => {}
        }
        s.push(' ');
    }
    s
}

/// Every character a document uses, in order of first use.
pub fn chars_of(blocks: &[Block]) -> Vec<char> {
    let mut seen = HashSet::new();
    text_of(blocks).chars().filter(|c| seen.insert(*c)).collect()
}

/// How many times each character occurs in a document.
pub fn char_counts(blocks: &[Block]) -> HashMap<char, usize> {
    let mut m = HashMap::new();
    for c in text_of(blocks).chars() {
        *m.entry(c).or_insert(0) += 1;
    }
    m
}

/// Characters among `chars` the person has not written (a letter shaped the same in the other
/// script counts: Latin a for Cyrillic а). Spaces and formatting marks are not characters.
pub fn missing(hand: &Hand, chars: impl IntoIterator<Item = char>) -> Vec<char> {
    let written: HashSet<&str> = hand.glyphs.iter().filter(|g| g.on && g.src != Src::Auto).map(|g| g.text.as_str()).collect();
    let mut seen = HashSet::new();
    chars
        .into_iter()
        .filter(|&c| !c.is_whitespace() && !crate::ingest::is_marker(c) && seen.insert(c))
        .filter(|&c| !charset::candidates(c).iter().any(|k| written.contains(k.to_string().as_str())))
        .collect()
}

/// Remove made-up versions (from earlier versions of the app); how many went.
pub fn drop_made(hand: &mut Hand) -> usize {
    let ids: Vec<String> = hand.glyphs.iter().filter(|g| g.src == Src::Auto).map(|g| g.id.clone()).collect();
    for id in &ids {
        hand.remove_glyph(id);
    }
    if !ids.is_empty() {
        hand.refresh();
    }
    ids.len()
}
