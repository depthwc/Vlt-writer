//! Characters per language, how many versions of each to collect, aliases and text normalisation.

use std::collections::HashSet;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

pub const LATIN_LOWER: &str = "abcdefghijklmnopqrstuvwxyz";
pub const LATIN_UPPER: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
pub const CYR_LOWER: &str = "абвгдеёжзийклмнопрстуфхцчшщъыьэюяўқғҳ";
pub const CYR_UPPER: &str = "АБВГДЕЁЖЗИЙКЛМНОПРСТУФХЦЧШЩЪЫЬЭЮЯЎҚҒҲ";
pub const DIGITS: &str = "0123456789";
pub const PUNCT: &str = ".,:;!?-()\"'ʻ«»№%/+=";
pub const MATH: &str = "×÷±√∑π∞≤≥≠≈°<>[]{}∫αβγΔθλμσφω→·";

/// What a person writes in. Decides which letters, pairs and copy lines the sheet asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lang {
    UzLatn,
    UzCyrl,
    Ru,
    En,
    Math,
}

impl Lang {
    pub const ALL: [Lang; 5] = [Lang::UzLatn, Lang::UzCyrl, Lang::Ru, Lang::En, Lang::Math];

    pub fn key(self) -> &'static str {
        match self {
            Lang::UzLatn => "uz_latn",
            Lang::UzCyrl => "uz_cyrl",
            Lang::Ru => "ru",
            Lang::En => "en",
            Lang::Math => "math",
        }
    }

    pub fn bit(self) -> u16 {
        1 << Lang::ALL.iter().position(|l| *l == self).unwrap()
    }

    /// Lowercase letters, most frequent first.
    pub fn letters(self) -> &'static str {
        match self {
            Lang::UzLatn => "aiernlosdkmubtyhqgzvxjpfcw",
            Lang::En => "etaoinshrdlcumwfgypbvkjxqz",
            Lang::Ru => "оеаинтсрвлкмдпуяыьгзбчйхжшюцщэфъё",
            Lang::UzCyrl => "аиронлесдкмубтйҳқгзўвхжпфғшчяюёэцъь",
            Lang::Math => "",
        }
    }

    pub fn latin(self) -> bool {
        matches!(self, Lang::UzLatn | Lang::En)
    }

    pub fn cyrillic(self) -> bool {
        matches!(self, Lang::Ru | Lang::UzCyrl)
    }

    /// Letter pairs written joined (ligatures), most useful first.
    pub fn pairs(self) -> &'static [&'static str] {
        match self {
            Lang::UzLatn => &["sh", "ch", "ng", "oʻ", "gʻ", "ar", "an", "la", "ni", "ir", "da", "ga", "li", "ka", "lar", "qa"],
            Lang::En => &["th", "he", "in", "er", "an", "re", "on", "at", "en", "nd", "ou", "ed", "ha", "st", "ng", "ll"],
            Lang::Ru => &["ст", "но", "то", "на", "ен", "ов", "ни", "ра", "ко", "ро", "ал", "ли", "по", "ре", "ть", "ла"],
            Lang::UzCyrl => &["ла", "ар", "ни", "ан", "ри", "да", "га", "ли", "ка", "қа", "ҳа", "ўз", "ғо", "ши", "чи", "лар"],
            Lang::Math => &[],
        }
    }

    /// Lines to copy: everyday sentences that together use every letter of the language.
    pub fn lines(self) -> &'static [&'static str] {
        match self {
            Lang::UzLatn => &[
                "Bugun ertalab biz qishloqqa bordik va",
                "oʻrmon yonidagi gʻalla dalasini koʻrdik.",
                "Chumchuqlar sayrab, shamol esar edi.",
                "Xonamizga yangi jihoz va kitoblar keldi.",
                "Vaqtni foydali oʻtkazish juda muhim.",
                "Jasur va Gulnoza hovlida oʻynashdi.",
                "Ertaga maktabda matematika darsi boʻladi.",
                "Fermer bogʻda olma, uzum va anor terdi.",
                "Toshkent shahri juda katta va goʻzal.",
                "Har bir inson oʻz ona tilini sevadi.",
                "Qishda qor yogʻib, yoʻllar oppoq boʻldi.",
                "Zarina yangi xabarni ovoz chiqarib oʻqidi.",
            ],
            Lang::UzCyrl => &[
                "Ўзбекистон ҳақида қизиқ ғоялар бор.",
                "Бугун эрталаб биз қишлоққа бордик ва",
                "ўрмон ёнидаги ғалла даласини кўрдик.",
                "Чумчуқлар сайраб, шамол эсар эди.",
                "Хонамизга янги жиҳоз ва китоблар келди.",
                "Вақтни фойдали ўтказиш жуда муҳим.",
                "Жасур ва Гулноза ҳовлида ўйнашди.",
                "Эртага мактабда математика дарси бўлади.",
                "Фермер боғда олма, узум ва анор терди.",
                "Юлдузлар осмонда ёрқин порлади.",
                "Санъат ва цирк ҳақида компьютерда ўқидик.",
                "Қишда қор ёғиб, йўллар оппоқ бўлди.",
            ],
            Lang::Ru => &[
                "Съешь же ещё этих мягких французских",
                "булок, да выпей чаю. В чащах юга жил бы",
                "цитрус? Да, но фальшивый экземпляр!",
                "Мы писали письмо другу и ждали ответа.",
                "Шёл снег, и дети катались на санках.",
                "Утром в парке было тихо и свежо.",
                "Широкая электрификация южных губерний",
                "даст мощный толчок подъёму хозяйства.",
                "Завтра у нас контрольная по истории.",
                "Бабушка испекла вкусный яблочный пирог.",
                "Книги помогают нам лучше понять мир.",
                "Ёжик нашёл в лесу большой гриб.",
            ],
            Lang::En => &[
                "The quick brown fox jumps over the lazy dog.",
                "Pack my box with five dozen liquor jugs.",
                "We went to the market early this morning.",
                "She wrote a long letter to her best friend.",
                "How vexingly quick daft zebras jump!",
                "My homework is due on Thursday at noon.",
                "The weather was warm and the sky was clear.",
                "Please bring your notebook and a pen.",
                "Every student read the chapter twice.",
                "Grandpa told us stories about the war.",
                "Bright vixens jump; dozy fowl quack.",
                "Jack quietly moved up front.",
            ],
            Lang::Math => &[],
        }
    }
}

/// How much to write: more versions per letter look more natural.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Quick,
    #[default]
    Normal,
    Best,
}

impl Level {
    pub const ALL: [Level; 3] = [Level::Quick, Level::Normal, Level::Best];

    /// Versions of a letter with frequency `rank` (0 = most frequent).
    pub fn letter_versions(self, rank: usize, upper: bool) -> usize {
        let t = if upper { 3 } else if rank < 10 { 0 } else if rank < 20 { 1 } else { 2 };
        let table: [usize; 4] = match self {
            Level::Quick => [3, 2, 1, 1],
            Level::Normal => [6, 4, 3, 2],
            Level::Best => [8, 6, 4, 3],
        };
        table[t]
    }

    pub fn symbol_versions(self, c: char) -> usize {
        let common = c.is_ascii_digit() || c == '.' || c == ',';
        match (self, common) {
            (Level::Quick, true) => 2,
            (Level::Quick, false) => 1,
            (Level::Normal, true) => 4,
            (Level::Normal, false) => 2,
            (Level::Best, true) => 5,
            (Level::Best, false) => 3,
        }
    }

    /// (pairs, versions of each pair, copy lines) per language.
    pub fn words(self) -> (usize, usize, usize) {
        match self {
            Level::Quick => (8, 1, 4),
            Level::Normal => (16, 2, 8),
            Level::Best => (16, 3, 12),
        }
    }
}

/// Languages a text is written in, from its letters: Cyrillic with ў қ ғ ҳ is Uzbek, other
/// Cyrillic Russian; Latin with q, x or ʻ is Uzbek, other Latin English.
pub fn guess_langs(text: &str) -> Vec<Lang> {
    let text = normalize(text).to_lowercase();
    let (mut lat, mut cyr, mut uz_l, mut uz_c) = (0usize, 0usize, 0usize, 0usize);
    for c in text.chars() {
        if c.is_ascii_lowercase() {
            lat += 1;
            if c == 'q' || c == 'x' {
                uz_l += 1;
            }
        } else if c == 'ʻ' {
            uz_l += 1;
        } else if CYR_LOWER.contains(c) {
            cyr += 1;
            if "ўқғҳ".contains(c) {
                uz_c += 1;
            }
        }
    }
    let mut out = vec![];
    if lat >= 20 {
        out.push(if uz_l * 100 >= lat { Lang::UzLatn } else { Lang::En });
    }
    if cyr >= 20 {
        out.push(if uz_c > 0 { Lang::UzCyrl } else { Lang::Ru });
    }
    out
}

/// A paragraph in English (most people print English): its words and letters say so more
/// than Uzbek ones (q, x, oʻ, va, bilan…). Too little text says nothing.
pub fn is_english(text: &str) -> bool {
    let t = normalize(text).to_lowercase();
    let latin = t.chars().filter(|c| c.is_ascii_lowercase()).count();
    if latin < 8 || t.chars().filter(|c| CYR_LOWER.contains(*c)).count() > latin {
        return false;
    }
    let cs: Vec<char> = t.chars().collect();
    let (mut en, mut uz) = (0usize, 0usize);
    for (i, &c) in cs.iter().enumerate() {
        let next = cs.get(i + 1).copied();
        match c {
            'w' => en += 1,
            'c' if next != Some('h') => en += 1,
            't' if next == Some('h') => en += 1,
            'q' | 'x' | 'ʻ' | TUTUQ => uz += 1,
            _ => {}
        }
    }
    for w in t.split(|c: char| !c.is_alphabetic() && c != 'ʻ').filter(|w| !w.is_empty()) {
        if ["the", "and", "of", "to", "is", "in", "it", "you", "that", "with", "for", "are", "this", "was", "be", "on"].contains(&w) {
            en += 2;
        }
        if ["va", "bu", "bilan", "uchun", "ham", "bir", "edi", "emas", "esa", "deb", "yoki"].contains(&w) || w.ends_with("lar") || w.ends_with("ning") || w.ends_with("dagi") {
            uz += 2;
        }
    }
    en >= 2 && en > 2 * uz
}

/// Punctuation a person needs for their languages.
pub fn punct_for(langs: &[Lang]) -> Vec<char> {
    let mut s = String::from(".,:;!?-()\"'%/+=");
    if langs.contains(&Lang::UzLatn) {
        s.push('ʻ');
        s.push(TUTUQ);
    }
    if langs.iter().any(|l| l.cyrillic()) {
        s.push_str("«»№");
    }
    s.chars().collect()
}

/// Cyrillic letters written like a Latin letter share one bank. Only pairs written alike in
/// cursive too: a cursive Т (like a Ш) is nothing like a cursive T.
pub const ALIAS: &[(char, char)] = &[
    ('а', 'a'), ('е', 'e'), ('о', 'o'), ('р', 'p'), ('с', 'c'), ('у', 'y'), ('х', 'x'),
    ('А', 'A'), ('В', 'B'), ('Е', 'E'), ('К', 'K'), ('М', 'M'), ('Н', 'H'), ('О', 'O'),
    ('Р', 'P'), ('С', 'C'), ('Х', 'X'),
];

pub fn alias_of(c: char) -> Option<char> {
    ALIAS.iter().find(|a| a.0 == c).map(|a| a.1)
}
pub fn alias_back(c: char) -> Option<char> {
    ALIAS.iter().find(|a| a.1 == c).map(|a| a.0)
}

pub fn upper(c: char) -> char {
    c.to_uppercase().next().unwrap_or(c)
}

static FREQUENT: LazyLock<HashSet<char>> = LazyLock::new(|| "aioenlrstukmdybqhgx иентвлкмдпяыьгзбч".chars().collect());
/// Letters that stay within the x-height (cursive в rises above it, like b).
pub static X_HEIGHT: LazyLock<HashSet<char>> = LazyLock::new(|| "acemnorsuvwxzагезиклмнопрстухцчшыьэюя".chars().collect());
pub static LETTERS: LazyLock<HashSet<char>> =
    LazyLock::new(|| [LATIN_LOWER, LATIN_UPPER, CYR_LOWER, CYR_UPPER].concat().chars().collect());

pub fn is_letter(c: char) -> bool {
    LETTERS.contains(&c)
}

/// Letter groups a person needs, in sheet order: (group, letters lowercase then uppercase, rank).
/// When both scripts are needed, Cyrillic letters shaped like Latin ones are collected once,
/// as Latin.
pub fn letters_for(langs: &[Lang]) -> Vec<(&'static str, Vec<(char, usize)>)> {
    let latin = langs.iter().any(|l| l.latin());
    let cyr = langs.iter().any(|l| l.cyrillic());
    let rank = |c: char| -> usize {
        let mut best = usize::MAX;
        for l in langs {
            for k in [Some(c), alias_of(c), alias_back(c)].into_iter().flatten() {
                if let Some(i) = l.letters().chars().position(|x| x == k) {
                    best = best.min(i);
                }
            }
        }
        best
    };
    let mut out = vec![];
    for (group, on, lower) in [("latin", latin, LATIN_LOWER), ("cyrillic", cyr, CYR_LOWER)] {
        if !on {
            continue;
        }
        let used: Vec<char> = lower
            .chars()
            .filter(|&c| langs.iter().any(|l| l.letters().contains(c)))
            .filter(|&c| !(group == "cyrillic" && latin && alias_of(c).is_some()))
            .collect();
        let mut v: Vec<(char, usize)> = used.iter().map(|&c| (c, rank(c))).collect();
        v.extend(used.iter().map(|&c| upper(c)).filter(|&u| !(group == "cyrillic" && latin && alias_of(u).is_some())).map(|u| (u, usize::MAX)));
        out.push((group, v));
    }
    out
}

/// Every character a person's languages need: (coverage group, characters).
pub fn target_chars(langs: &[Lang]) -> Vec<(&'static str, Vec<char>)> {
    let langs: Vec<Lang> = if langs.is_empty() { Lang::ALL.to_vec() } else { langs.to_vec() };
    let latin = langs.iter().any(|l| l.latin());
    let mut out: Vec<(&'static str, Vec<char>)> = letters_for(&langs)
        .into_iter()
        .map(|(g, v)| {
            let mut cs: Vec<char> = v.into_iter().map(|x| x.0).collect();
            if g == "cyrillic" && latin {
                // shown too: they count through the shared Latin bank
                cs.extend(ALIAS.iter().map(|a| a.0).filter(|&c| langs.iter().any(|l| l.cyrillic() && (l.letters().contains(c) || l.letters().contains(c.to_lowercase().next().unwrap())))));
            }
            (g, cs)
        })
        .collect();
    let mut sym: Vec<char> = DIGITS.chars().collect();
    sym.extend(punct_for(&langs));
    out.push(("digits", sym));
    if langs.contains(&Lang::Math) {
        out.push(("math", MATH.chars().collect()));
    }
    out
}

// ------------------------------------------------------------------ first sheet (v0.2)
// Printed sheets of version 0.2 carry page numbers of this fixed layout; keep it unchanged.

pub const UZ_PAIRS: &[&str] = &["sh", "ch", "ng", "ar", "an", "la", "ni", "ir", "in", "ga", "da", "li", "ri", "ka", "lar", "bo", "ol", "yo", "qa"];
pub const RU_PAIRS: &[&str] = &["ст", "но", "то", "на", "ен", "ов", "ни", "ра", "ко", "ро", "ал", "ли", "по", "ло", "ре", "ол", "ла", "от", "ва", "ть"];

pub const TEXT_LINES: &[&str] = &[
    "Bugun ertalab biz qishloqqa bordik va",
    "oʻrmon yonidagi gʻalla dalasini koʻrdik.",
    "Chumchuqlar sayrab, shamol esar edi.",
    "Xonamizga yangi jihoz va kitoblar keldi.",
    "Vaqtni foydali oʻtkazish juda muhim.",
    "Jasur va Gulnoza hovlida oʻynashdi.",
    "Ertaga maktabda matematika darsi boʻladi.",
    "Съешь же ещё этих мягких французских",
    "булок, да выпей чаю. В чащах юга жил бы",
    "цитрус? Да, но фальшивый экземпляр!",
    "Мы писали письмо другу и ждали ответа.",
    "Шёл снег, и дети катались на санках.",
    "Ўзбекистон ҳақида қизиқ ғоялар бор.",
    "The quick brown fox jumps over the lazy dog.",
    "12 + 35 = 47, 2024 yil, 80% va №6.",
];

/// Lines on the digits page of every new sheet.
pub const DIGIT_LINES: &[&str] = &["12 + 35 = 47, 90 - 15 = 75, 3/4", "2024, 80%, (7.5), 6:30, 100!"];

/// Versions per character on the first sheet.
pub fn variants_for(c: char) -> usize {
    if c == 'ё' || c == 'Ё' {
        2
    } else if c.is_lowercase() {
        if FREQUENT.contains(&c) { 6 } else { 4 }
    } else if c.is_uppercase() {
        2
    } else if c.is_ascii_digit() || c == '.' || c == ',' {
        4
    } else {
        2
    }
}

/// Unicode NFC; Uzbek oʻ / gʻ with U+02BB (the mark is part of those letters) and the tutuq
/// belgisi between letters (taʼlim, sanʼat) with U+02BC, a sign of its own; tabs, ellipsis.
pub fn normalize(text: &str) -> String {
    const APOS: &[char] = &['\'', '’', 'ʼ', '`', 'ʻ', '‘', '´'];
    let s: String = text.nfc().collect();
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    for (i, &c) in chars.iter().enumerate() {
        if APOS.contains(&c) && i > 0 && matches!(chars[i - 1], 'o' | 'O' | 'g' | 'G') {
            out.push('ʻ');
        } else if APOS.contains(&c) && i > 0 && chars[i - 1].is_alphabetic() && chars.get(i + 1).is_some_and(|n| n.is_alphabetic()) {
            out.push(TUTUQ);
        } else if c == '\t' {
            out.push_str("    ");
        } else if c == '\u{a0}' || c == '\u{202f}' {
            out.push(' ');
        } else if c == '…' {
            out.push_str("...");
        } else {
            out.push(c);
        }
    }
    out
}

/// The Uzbek tutuq belgisi (taʼlim, maʼno): a sign of its own, not part of the letter before.
pub const TUTUQ: char = 'ʼ';

/// The same character in the other script (a / а), if any.
pub fn same(c: char) -> Vec<char> {
    let mut v = vec![c];
    v.extend(alias_of(c));
    v.extend(alias_back(c));
    v
}

/// Bank keys to try for one character, best first: itself, its other-script twin, then marks
/// that look the same in handwriting (quotes, apostrophes, dashes).
pub fn candidates(c: char) -> Vec<char> {
    let mut v = same(c);
    let fb: &[char] = match c {
        '’' | 'ʼ' | '´' => &['\'', 'ʻ'],
        '`' | '‘' => &['ʻ'],
        '“' | '”' | '„' => &['"'],
        '—' | '–' | '−' | '‐' => &['-'],
        '\'' => &['ʼ', 'ʻ'],
        'ʻ' => &['\'', 'ʼ'],
        _ => &[],
    };
    for f in fb {
        v.extend(same(*f));
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uzbek_apostrophes() {
        assert_eq!(normalize("o'qidim g‘alaba ma'no 'san'at'"), "oʻqidim gʻalaba maʼno 'sanʼat'");
        assert!(candidates('а').contains(&'a'));
        assert_eq!(variants_for('a'), 6);
    }

    #[test]
    fn guesses_languages() {
        assert_eq!(guess_langs("Xulosa qilib aytganda, Islom qadriyatlari nafaqat tarixiy"), vec![Lang::UzLatn]);
        assert_eq!(guess_langs("The weather was warm and the sky was clear today."), vec![Lang::En]);
        assert_eq!(guess_langs("Мы писали письмо другу и ждали ответа."), vec![Lang::Ru]);
        assert_eq!(guess_langs("Ўзбекистон ҳақида қизиқ ғоялар бор, дедик."), vec![Lang::UzCyrl]);
        // a paragraph to print: English, even with a q or an x in it
        assert!(is_english("The quick brown fox jumps over the lazy dog."));
        assert!(!is_english("Xulosa qilib aytganda, Islom qadriyatlari nafaqat tarixiy"));
        assert!(!is_english("Bugun biz maktabga bordik va dars qildik."));
        assert!(!is_english("Hi"));
    }

    #[test]
    fn letters_per_language() {
        let uz = letters_for(&[Lang::UzLatn]);
        assert_eq!(uz.len(), 1);
        assert_eq!(uz[0].1.len(), 52);
        // Russian alone: every Cyrillic letter, aliased ones included
        let ru = letters_for(&[Lang::Ru]);
        assert!(ru[0].1.iter().any(|x| x.0 == 'а') && !ru[0].1.iter().any(|x| x.0 == 'ў'));
        // both scripts: а is collected once, as Latin a
        let both = letters_for(&[Lang::UzLatn, Lang::Ru]);
        assert!(!both[1].1.iter().any(|x| x.0 == 'а' || x.0 == 'А'));
        assert!(both[1].1.iter().any(|x| x.0 == 'Б'));
        for l in Lang::ALL {
            for line in l.lines() {
                assert!(line.chars().count() <= 45, "{line}");
            }
        }
        // copy lines use every letter of their language
        for l in [Lang::UzLatn, Lang::UzCyrl, Lang::Ru, Lang::En] {
            let all: String = l.lines().concat().to_lowercase();
            let skip = if l == Lang::UzLatn { "cw" } else { "" };
            for c in l.letters().chars().filter(|c| !skip.contains(*c)) {
                assert!(all.contains(c), "{l:?} lines miss {c}");
            }
        }
    }
}
