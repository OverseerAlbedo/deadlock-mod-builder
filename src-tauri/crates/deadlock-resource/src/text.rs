//! Lossless text KV3: offsets are UTF-8 byte offsets; untouched source stays exact.
use crate::{invalid, Result, MAX_BLOCK, MAX_COUNT, MAX_DEPTH};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub start: usize,
    pub end: usize,
    pub kind: NodeKind,
}
#[derive(Clone, Debug, PartialEq)]
pub enum NodeKind {
    Scalar { value: String, quoted: bool },
    Blob(Vec<u8>),
    Array(Vec<Node>),
    Object(Vec<(String, Node)>),
    Typed { name: String, value: Box<Node> },
}
impl Node {
    pub fn get(&self, key: &str) -> Option<&Node> {
        match &self.kind {
            NodeKind::Object(rows) => rows.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    pub fn scalar(&self) -> Option<&str> {
        match &self.kind {
            NodeKind::Scalar { value, .. } => Some(value),
            _ => None,
        }
    }
    pub fn object(&self) -> Option<&[(String, Node)]> {
        match &self.kind {
            NodeKind::Object(v) => Some(v),
            _ => None,
        }
    }
    pub fn array(&self) -> Option<&[Node]> {
        match &self.kind {
            NodeKind::Array(v) => Some(v),
            _ => None,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub start: usize,
    pub end: usize,
    pub replacement: String,
}
#[derive(Clone, Debug)]
pub struct Document {
    pub source: String,
    pub root: Node,
}
#[derive(Clone, Copy)]
struct Token {
    start: usize,
    end: usize,
}
fn tokens(text: &str) -> Result<Vec<Token>> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < text.len() {
        let tail = &text[at..];
        let c = tail
            .chars()
            .next()
            .ok_or_else(|| invalid("invalid token"))?;
        if c.is_whitespace() {
            at += c.len_utf8();
            continue;
        }
        if tail.starts_with("<!--") || tail.starts_with("/*") {
            let (open, close) = if tail.starts_with("<!--") {
                (4, "-->")
            } else {
                (2, "*/")
            };
            let n = tail[open..]
                .find(close)
                .ok_or_else(|| invalid("unterminated comment"))?;
            at += open + n + close.len();
            continue;
        }
        if tail.starts_with("//") {
            at += tail.find('\n').unwrap_or(tail.len());
            continue;
        }
        let start = at;
        if tail.starts_with("\"\"\"") {
            let n = tail[3..]
                .find("\"\"\"")
                .ok_or_else(|| invalid("unterminated multiline string"))?;
            at += n + 6;
        } else if c == '"' {
            at += 1;
            let mut escaped = false;
            let mut ended = false;
            while at < text.len() {
                let c = text[at..]
                    .chars()
                    .next()
                    .ok_or_else(|| invalid("invalid string"))?;
                at += c.len_utf8();
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == '"' {
                    ended = true;
                    break;
                }
            }
            if !ended {
                return Err(invalid("unterminated string"));
            }
        } else if "{}[]=,:".contains(c) {
            at += 1;
        } else {
            while at < text.len() {
                let c = text[at..]
                    .chars()
                    .next()
                    .ok_or_else(|| invalid("invalid token"))?;
                if c.is_whitespace() || "{}[]=,:".contains(c) {
                    break;
                }
                at += c.len_utf8();
            }
        }
        out.push(Token { start, end: at });
        if out.len() > 8 * MAX_COUNT {
            return Err(invalid("text token count exceeds bound"));
        }
    }
    Ok(out)
}
struct Parser<'a> {
    source: &'a str,
    tokens: Vec<Token>,
    at: usize,
}
impl<'a> Parser<'a> {
    fn peek(&self) -> Option<&str> {
        self.tokens
            .get(self.at)
            .map(|t| &self.source[t.start..t.end])
    }
    fn take(&mut self, expected: Option<&str>) -> Result<Token> {
        let token = *self
            .tokens
            .get(self.at)
            .ok_or_else(|| invalid("unexpected end of text document"))?;
        if expected.is_some_and(|want| want != &self.source[token.start..token.end]) {
            return Err(invalid(format!(
                "expected {}, found {}",
                expected.unwrap_or_default(),
                &self.source[token.start..token.end]
            )));
        }
        self.at += 1;
        Ok(token)
    }
    fn scalar(raw: &str) -> Result<(String, bool)> {
        if raw.starts_with("\"\"\"") {
            Ok((raw[3..raw.len() - 3].into(), true))
        } else if raw.starts_with('"') {
            Ok((
                serde_json::from_str(raw).map_err(|_| invalid("invalid quoted string"))?,
                true,
            ))
        } else {
            Ok((raw.into(), false))
        }
    }
    fn value(&mut self, depth: usize) -> Result<Node> {
        if depth > MAX_DEPTH {
            return Err(invalid("text nesting exceeds supported bound"));
        }
        let token = self.take(None)?;
        let raw = &self.source[token.start..token.end];
        let mut end = token.end;
        let kind = match raw {
            "#" => {
                self.take(Some("["))?;
                let mut bytes = Vec::new();
                while self.peek() != Some("]") {
                    let part = self.take(None)?;
                    let hex = &self.source[part.start..part.end];
                    if hex.is_empty()
                        || hex.len() % 2 != 0
                        || !hex.bytes().all(|v| v.is_ascii_hexdigit())
                    {
                        return Err(invalid("invalid hexadecimal bytes in binary blob"));
                    }
                    for i in (0..hex.len()).step_by(2) {
                        bytes.push(
                            u8::from_str_radix(&hex[i..i + 2], 16)
                                .map_err(|_| invalid("invalid blob"))?,
                        );
                    }
                }
                end = self.take(Some("]"))?.end;
                NodeKind::Blob(bytes)
            }
            "{" => {
                let mut members = Vec::new();
                let mut names = HashSet::new();
                while self.peek() != Some("}") {
                    let key = self.take(None)?;
                    let key = &self.source[key.start..key.end];
                    if key == "," {
                        continue;
                    }
                    let (key, _) = Self::scalar(key)?;
                    self.take(Some("="))?;
                    if !names.insert(key.clone()) {
                        return Err(invalid(format!("duplicate text key: {key}")));
                    }
                    members.push((key, self.value(depth + 1)?));
                    if members.len() > MAX_COUNT {
                        return Err(invalid("object count exceeds bound"));
                    }
                }
                end = self.take(Some("}"))?.end;
                NodeKind::Object(members)
            }
            "[" => {
                let mut values = Vec::new();
                while self.peek() != Some("]") {
                    if self.peek() == Some(",") {
                        self.take(None)?;
                    } else {
                        values.push(self.value(depth + 1)?);
                    }
                    if values.len() > MAX_COUNT {
                        return Err(invalid("array count exceeds bound"));
                    }
                }
                end = self.take(Some("]"))?.end;
                NodeKind::Array(values)
            }
            _ => {
                if self.peek() == Some(":") {
                    self.take(Some(":"))?;
                    let sub = self.value(depth + 1)?;
                    end = sub.end;
                    NodeKind::Typed {
                        name: raw.into(),
                        value: Box::new(sub),
                    }
                } else {
                    let (value, quoted) = Self::scalar(raw)?;
                    NodeKind::Scalar { value, quoted }
                }
            }
        };
        Ok(Node {
            start: token.start,
            end,
            kind,
        })
    }
}
impl Document {
    pub fn parse(source: impl Into<String>) -> Result<Self> {
        let source = source.into();
        if source.len() > MAX_BLOCK {
            return Err(invalid("text document exceeds 128 MiB"));
        }
        let mut parser = Parser {
            tokens: tokens(&source)?,
            source: &source,
            at: 0,
        };
        let root = parser.value(0)?;
        if parser.at != parser.tokens.len() || !matches!(root.kind, NodeKind::Object(_)) {
            return Err(invalid("unsupported or malformed text document"));
        }
        Ok(Self { source, root })
    }
    pub fn objects(&self, class_name: Option<&str>) -> Vec<&Node> {
        fn walk<'a>(node: &'a Node, class: Option<&str>, out: &mut Vec<&'a Node>) {
            match &node.kind {
                NodeKind::Object(rows) => {
                    if class.is_none() || node.get("_class").and_then(Node::scalar) == class {
                        out.push(node);
                    }
                    for (_, n) in rows {
                        walk(n, class, out);
                    }
                }
                NodeKind::Array(values) => {
                    for n in values {
                        walk(n, class, out);
                    }
                }
                _ => (),
            }
        }
        let mut out = Vec::new();
        walk(&self.root, class_name, &mut out);
        out
    }
    pub fn apply(&self, edits: &[Edit]) -> Result<String> {
        let mut edits = edits.to_vec();
        edits.sort_by(|a, b| {
            (&b.start, &b.end, &b.replacement).cmp(&(&a.start, &a.end, &a.replacement))
        });
        let mut output = self.source.clone();
        let mut previous = self.source.len() + 1;
        for edit in edits {
            if edit.start > edit.end
                || edit.end > self.source.len()
                || edit.end > previous
                || !self.source.is_char_boundary(edit.start)
                || !self.source.is_char_boundary(edit.end)
            {
                return Err(invalid("overlapping or invalid source edits refused"));
            }
            output.replace_range(edit.start..edit.end, &edit.replacement);
            previous = edit.start;
        }
        Self::parse(output.clone())?;
        Ok(output)
    }
}
pub fn field_edit(node: &Node, key: &str, value: &str) -> Result<Edit> {
    if node.object().is_none() {
        return Err(invalid("field edit requires object"));
    }
    let encoded = serde_json::to_string(value).map_err(|_| invalid("string encoding failed"))?;
    Ok(if let Some(old) = node.get(key) {
        Edit {
            start: old.start,
            end: old.end,
            replacement: encoded,
        }
    } else {
        Edit {
            start: node.end - 1,
            end: node.end - 1,
            replacement: format!("\n\t\t{key} = {encoded}\n\t"),
        }
    })
}
