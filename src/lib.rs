#![feature(custom_derive, plugin)]
#![plugin(serde_macros)]

extern crate serde;
extern crate serde_json;

use serde_json::Value;

use std::collections::BTreeMap;
use std::process::{Command, Stdio};
use std::io::{Write, Read, Error};

trait Walkable<T> {
    fn walk<F>(self, f: &F) -> Self
        where F : Fn(T) -> T;
}

impl<T> Walkable<T> for T {
    fn walk<F>(self, f: &F) -> Self
        where F : Fn(Self) -> Self {
        f(self)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Pandoc {
    pub meta: Meta,
    pub blocks: Vec<Block>
}

impl Pandoc {
    pub fn new(meta: Value, blocks: Value) -> Self {
        let converted_meta = serde_json::from_value(meta).unwrap();
        let converted_blocks = serde_json::from_value(convert_entry(blocks)).unwrap();
        Pandoc { meta: converted_meta,
                 blocks: converted_blocks
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Meta {
    #[serde(rename = "unMeta")]
    pub un_meta: BTreeMap<String, MetaValue>
}

#[derive(Debug, Serialize, Deserialize)]
pub enum MetaValue {
    MetaMap(BTreeMap<String, MetaValue>),
    MetaList(Vec<MetaValue>),
    MetaBool(bool),
    MetaString(String),
    MetaInlines(Vec<Inline>),
    MetaBlocks(Vec<Block>)
}

// http://hackage.haskell.org/package/pandoc-types-1.16.1.1/docs/Text-Pandoc-Definition.html#t:Block
#[derive(PartialEq, Debug, Serialize, Deserialize)]
pub enum Block {
    Plain(Vec<Inline>),
    Para(Vec<Inline>),
    CodeBlock(Attr, String),
    RawBlock(Format, String),
    BlockQuote(Vec<Block>),
    OrderedList(ListAttributes, Vec<Vec<Block>>),
    BulletList(Vec<Vec<Block>>),
    DefinitionList(Vec<(Vec<Inline>, Vec<Vec<Block>>)>),
    Header(u64, Attr, Vec<Inline>),
    HorizontalRule,
    Table(Vec<Inline>, Vec<Alignment>, Vec<f64>, Vec<TableCell>, Vec<Vec<TableCell>>),
    Div(Attr, Vec<Block>),
    Null
}

impl Walkable<Inline> for Block {
    fn walk<F>(self, f: &F) -> Self
        where F : Fn(Inline) -> Inline {
        match self {
            Block::Plain(inlines) => Block::Plain(inlines.walk(f)),
            Block::Para(inlines) => Block::Plain(inlines.walk(f)),
            Block::BlockQuote(blocks) =>
                Block::BlockQuote(blocks.walk(f)),
            Block::OrderedList(list, vecs_blocks) =>
                Block::OrderedList(list, vecs_blocks.walk(f)),
            Block::BulletList(vecs_blocks) =>
                Block::BulletList(vecs_blocks.walk(f)),
            Block::DefinitionList(inlines_and_blocks) => {
                Block::DefinitionList(inlines_and_blocks
                                      .into_iter()
                                      .map(|(is, vbs)| (is.walk(f), vbs.walk(f)))
                                      .collect())
            },
            Block::Header(i, attr, inlines) =>
                Block::Header(i, attr, inlines.walk(f)),
            Block::Table(inlines, alignment, width, headers, rows) => {
                Block::Table(inlines.walk(f), alignment, width,
                             headers.walk(f),
                             rows.walk(f))
            },
            Block::Div(attr, blocks) =>
                Block::Div(attr, blocks.walk(f)),
            b => b
        }
    }
}

impl<T> Walkable<Inline> for Vec<T>
    where T : Walkable<Inline> {
    fn walk<F>(self, f: &F) -> Self
        where F : Fn(Inline) -> Inline {
        self.into_iter().map(|i| i.walk(f)).collect()
    }
}

pub type ListAttributes = (u64, ListNumberStyle, ListNumberDelim);
#[derive(PartialEq, Debug, Serialize, Deserialize)]
pub enum ListNumberStyle {
    DefaultStyle,
    Example,
    Decimal,
    LowerRoman,
    UpperRoman,
    LowerAlpha,
    UpperAlpha
}

#[derive(PartialEq, Debug, Serialize, Deserialize)]
pub enum ListNumberDelim {
    DefaultDelim,
    Period,
    OneParen,
    TwoParens
}

#[derive(PartialEq, Debug, Serialize, Deserialize)]
pub enum Alignment {
    AlignLeft,
    AlignRight,
    AlignCenter,
    AlignDefault
}

type TableCell = Vec<Block>;

// http://hackage.haskell.org/package/pandoc-types-1.16.1.1/docs/Text-Pandoc-Definition.html#t:Inline
#[derive(PartialEq, Debug, Serialize, Deserialize)]
pub enum Inline {
    Str(String),
    Emph(Vec<Inline>),
    Strong(Vec<Inline>),
    Strikeout(Vec<Inline>),
    Superscript(Vec<Inline>),
    Subscript(Vec<Inline>),
    SmallCaps(Vec<Inline>),
    Quoted(QuoteType, Vec<Inline>),
    Cite(Vec<Citation>, Vec<Inline>),
    Code(Attr, String),
    Space,
    SoftBreak,
    LineBreak,
    Math(MathType, String),
    RawInline(Format, String),
    Link(Attr, Vec<Inline>, Target),
    Image(Attr, Vec<Inline>, Target),
    Span(Attr, Vec<Inline>)
}

#[derive(PartialEq, Debug, Serialize, Deserialize)]
pub enum QuoteType {
    SingleQuote,
    DoubleQuote
}

#[derive(PartialEq, Debug, Serialize, Deserialize)]
pub enum MathType {
    DisplayMath,
    InlineMath
}

pub type Format = String;
pub type Attr = (String, Vec<String>, Vec<(String, String)>);
pub type Target = (String, String);

#[derive(PartialEq, Debug, Serialize, Deserialize)]
pub struct Citation {
    #[serde(rename = "citationId")]
    citation_id: String,
    #[serde(rename = "citationPrefix")]
    citation_prefix: Vec<Inline>,
    #[serde(rename = "citationSuffix")]
    citation_suffix: Vec<Inline>,
    #[serde(rename = "citationmode")]
    citation_mode: CitationMode,
    #[serde(rename = "citationNoteNum")]
    citation_note_num: u64,
    #[serde(rename = "citationHash")]
    citation_hash: u64
}

#[derive(PartialEq, Debug, Serialize, Deserialize)]
pub enum CitationMode {
    AuthorInText,
    SuppressAuthor,
    NormalCitation
}

pub fn convert_entry(entry: Value) -> Value {
    match entry {
        Value::Object(obj) => {
            let mut new_entry = BTreeMap::new();
            let t = String::from(obj.get("t").unwrap().as_str().unwrap());
            let c = obj.get("c").unwrap().clone();
            let new_c = convert_entry(c);
            new_entry.insert(t, new_c);
            Value::Object(new_entry)
        },
        Value::Array(arr) => {
            let mut array = Vec::new();
            for item in arr {
                array.push(convert_entry(item));
            }
            Value::Array(array)
        },
        _ => entry
    }
}

pub fn to_json(markdown: String) -> Result<String, Error> {
    let process = try!(
        Command::new("pandoc")
            .args(&["-t", "json", "--mathjax"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
    );
    try!(process.stdin.unwrap().write_all(markdown.as_bytes()));

    let mut s = String::new();
    try!(process.stdout.unwrap().read_to_string(&mut s));
    Ok(s)
}

pub fn deserialize(markdown: String) -> Result<Pandoc, String> {
    let json = try!(to_json(markdown).map_err(|e| e.to_string()));
    let value: Value = try!(serde_json::from_str(&json).map_err(|e| e.to_string()));
    let arr: &Vec<Value> = try!(value.as_array().ok_or("Not an array"));

    if arr.len() != 2 {
        return Err(String::from("Not valid Pandoc"))
    }
    let pandoc = Pandoc::new(arr[0].clone(), arr[1].clone());
    Ok(pandoc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use Walkable;
    use serde_json;
    use std::collections::BTreeMap;

    #[test]
    fn test_convert_entry_object() {
        let mut map = BTreeMap::new();
        map.insert(String::from("Str"), serde_json::Value::String(String::from("Test")));
        let expected = serde_json::Value::Object(map);

        let mut object_builder = serde_json::builder::ObjectBuilder::new();
        object_builder = object_builder.insert(String::from("t"), String::from("Str"));
        object_builder = object_builder.insert(String::from("c"), String::from("Test"));
        let object = object_builder.build();
        let converted = convert_entry(object);

        assert_eq!(converted, expected);
    }

    #[test]
    fn test_convert_entry_array() {
        let arr = vec![serde_json::Value::String(String::from("Test")),
                           serde_json::Value::String(String::from("string"))];
        let expected = serde_json::Value::Array(arr);

        let mut array_builder = serde_json::builder::ArrayBuilder::new();
        array_builder = array_builder.push(String::from("Test"));
        array_builder = array_builder.push(String::from("string"));
        let array = array_builder.build();
        let converted = convert_entry(array);

        assert_eq!(converted, expected);
    }

    #[test]
    fn test_deserialize() {
        let markdown = String::from("# Test");
        let pandoc = deserialize(markdown).unwrap();
        assert!(pandoc.meta.un_meta.is_empty());
        assert_eq!(pandoc.blocks, vec![
            Block::Header(1, (String::from("test"), vec![], vec![]),
                          vec![Inline::Str(String::from("Test"))])
        ]);
    }

    #[test]
    fn test_to_json() {
        let json = to_json(String::from("# Test")).unwrap();
        let pandoc = r#"[{"unMeta":{}},[{"t":"Header","c":[1,["test",[],[]],[{"t":"Str","c":"Test"}]]}]]"#;
        let expected: serde_json::Value = serde_json::from_str(pandoc).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, expected);
    }

    #[test]
    fn test_walk_inline() {
        let inline_str = Inline::Str(String::from("a"));
        let emph = inline_str.walk(|s| Inline::Emph(vec![s]));
        let expected = Inline::Emph(vec![Inline::Str(String::from("a"))]);
        assert_eq!(emph, expected);
    }

}
