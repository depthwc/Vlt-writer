//! Tiny XML tree on top of quick-xml (documents are small; a tree keeps the readers simple).

use quick_xml::events::Event;
use quick_xml::Reader;

#[derive(Debug, Default, Clone)]
pub struct El {
    /// qualified name as written, e.g. "w:p"
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub kids: Vec<Node>,
}

#[derive(Debug, Clone)]
pub enum Node {
    El(El),
    Text(String),
}

impl El {
    pub fn attr(&self, k: &str) -> Option<&str> {
        self.attrs.iter().find(|(a, _)| a == k).map(|(_, v)| v.as_str())
    }
    pub fn els(&self) -> impl Iterator<Item = &El> {
        self.kids.iter().filter_map(|n| if let Node::El(e) = n { Some(e) } else { None })
    }
    pub fn child(&self, name: &str) -> Option<&El> {
        self.els().find(|e| e.name == name)
    }
    pub fn path(&self, names: &[&str]) -> Option<&El> {
        let mut cur = self;
        for n in names {
            cur = cur.child(n)?;
        }
        Some(cur)
    }
    /// All descendants (depth-first, document order) named `name`.
    pub fn find_all<'a>(&'a self, name: &str, out: &mut Vec<&'a El>) {
        for e in self.els() {
            if e.name == name {
                out.push(e);
            }
            e.find_all(name, out);
        }
    }
    pub fn text(&self) -> String {
        let mut s = String::new();
        for k in &self.kids {
            match k {
                Node::Text(t) => s.push_str(t),
                Node::El(e) => s.push_str(&e.text()),
            }
        }
        s
    }
}

pub fn parse(xml: &str) -> anyhow::Result<El> {
    let mut r = Reader::from_str(xml);
    r.config_mut().trim_text(false);
    let mut stack: Vec<El> = vec![El { name: "#root".into(), ..Default::default() }];
    loop {
        match r.read_event()? {
            Event::Start(e) => stack.push(el_from(&e)),
            Event::Empty(e) => {
                let el = el_from(&e);
                stack.last_mut().unwrap().kids.push(Node::El(el));
            }
            Event::End(_) => {
                let el = stack.pop().unwrap();
                if let Some(parent) = stack.last_mut() {
                    parent.kids.push(Node::El(el));
                } else {
                    stack.push(el);
                }
            }
            Event::Text(t) => {
                // entity references arrive as separate GeneralRef events, so text is literal
                let s: &str = t.as_ref();
                stack.last_mut().unwrap().kids.push(Node::Text(s.to_string()));
            }
            Event::GeneralRef(e) => {
                let name: String = AsRef::<str>::as_ref(&e).to_string();
                let s = match name.as_str() {
                    "amp" => "&".into(),
                    "lt" => "<".into(),
                    "gt" => ">".into(),
                    "quot" => "\"".into(),
                    "apos" => "'".into(),
                    n if n.starts_with("#x") => u32::from_str_radix(&n[2..], 16).ok().and_then(char::from_u32).map(String::from).unwrap_or_default(),
                    n if n.starts_with('#') => n[1..].parse::<u32>().ok().and_then(char::from_u32).map(String::from).unwrap_or_default(),
                    _ => String::new(),
                };
                stack.last_mut().unwrap().kids.push(Node::Text(s));
            }
            Event::CData(t) => {
                let s: &str = t.as_ref();
                stack.last_mut().unwrap().kids.push(Node::Text(s.to_string()));
            }
            Event::Eof => break,
            _ => {}
        }
    }
    while stack.len() > 1 {
        let el = stack.pop().unwrap();
        stack.last_mut().unwrap().kids.push(Node::El(el));
    }
    Ok(stack.pop().unwrap())
}

fn el_from(e: &quick_xml::events::BytesStart) -> El {
    let name = e.name().as_ref().to_string();
    #[allow(deprecated)]
    let attrs = e
        .attributes()
        .flatten()
        .map(|a| (a.key.as_ref().to_string(), a.unescape_value().map(|v| v.into_owned()).unwrap_or_default()))
        .collect();
    El { name, attrs, kids: vec![] }
}

pub fn zip_entry(data: &[u8], name: &str) -> Option<String> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(data)).ok()?;
    let mut f = z.by_name(name).ok()?;
    let mut s = String::new();
    std::io::Read::read_to_string(&mut f, &mut s).ok()?;
    Some(s)
}
