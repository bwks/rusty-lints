//! The little JSON `rusty-lints` reads and writes: it reads the diagnostics cargo prints with
//! `--message-format=json`, one value per line, and writes its report.

use std::fmt::Write;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Value>),
    Object(Vec<(String, Value)>),
}

struct Parser<'a> {
    text: &'a str,
    at: usize,
}

impl Value {
    /// Parses `text`, which holds one JSON value and nothing else but white space.
    pub fn parse(text: &str) -> Option<Value> {
        let mut parser = Parser { text, at: 0 };
        let value = parser.value()?;
        parser.skip_space();
        (parser.at == text.len()).then_some(value)
    }

    /// The value of `key`, when this is an object that has it.
    pub fn get(&self, key: &str) -> Option<&Value> {
        let Value::Object(members) = self else {
            return None;
        };
        members
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value)
    }

    /// The text, when this is a string.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(text) => Some(text),
            _ => None,
        }
    }

    /// The items, when this is an array; none otherwise.
    pub fn items(&self) -> &[Value] {
        match self {
            Value::Array(items) => items,
            _ => &[],
        }
    }

    /// The number, when this is a whole number that fits a `usize`.
    pub fn as_usize(&self) -> Option<usize> {
        let Value::Number(number) = *self else {
            return None;
        };
        if number.fract() != 0.0 || number < 0.0 || number > 2f64.powi(52) {
            return None;
        }
        format!("{number:.0}").parse().ok()
    }

    /// Whether this is `true`.
    pub fn is_true(&self) -> bool {
        matches!(self, Value::Bool(true))
    }
}

impl Parser<'_> {
    /// The value that starts at the cursor, after any white space.
    fn value(&mut self) -> Option<Value> {
        self.skip_space();
        match self.peek()? {
            b'{' => self.object(),
            b'[' => self.array(),
            b'"' => self.string().map(Value::String),
            b't' => self.word("true", Value::Bool(true)),
            b'f' => self.word("false", Value::Bool(false)),
            b'n' => self.word("null", Value::Null),
            _ => self.number(),
        }
    }

    /// The object at the cursor.
    fn object(&mut self) -> Option<Value> {
        self.at += 1;
        let mut members = Vec::new();
        self.skip_space();
        if self.eat(b'}') {
            return Some(Value::Object(members));
        }
        loop {
            self.skip_space();
            let key = self.string()?;
            self.skip_space();
            if !self.eat(b':') {
                return None;
            }
            members.push((key, self.value()?));
            self.skip_space();
            if self.eat(b'}') {
                return Some(Value::Object(members));
            }
            if !self.eat(b',') {
                return None;
            }
        }
    }

    /// The array at the cursor.
    fn array(&mut self) -> Option<Value> {
        self.at += 1;
        let mut items = Vec::new();
        self.skip_space();
        if self.eat(b']') {
            return Some(Value::Array(items));
        }
        loop {
            items.push(self.value()?);
            self.skip_space();
            if self.eat(b']') {
                return Some(Value::Array(items));
            }
            if !self.eat(b',') {
                return None;
            }
        }
    }

    /// The string at the cursor, its escapes resolved.
    fn string(&mut self) -> Option<String> {
        if !self.eat(b'"') {
            return None;
        }
        let mut text = String::new();
        loop {
            let rest = &self.text[self.at..];
            let end = rest.find(['"', '\\'])?;
            text.push_str(&rest[..end]);
            self.at += end + 1;
            if rest.as_bytes()[end] == b'"' {
                return Some(text);
            }
            let escape = self.peek()?;
            self.at += 1;
            match escape {
                b'"' => text.push('"'),
                b'\\' => text.push('\\'),
                b'/' => text.push('/'),
                b'b' => text.push('\u{8}'),
                b'f' => text.push('\u{c}'),
                b'n' => text.push('\n'),
                b'r' => text.push('\r'),
                b't' => text.push('\t'),
                b'u' => text.push(self.unicode_escape()?),
                _ => return None,
            }
        }
    }

    /// The character of a `\u` escape whose four hex digits start at the cursor, joining a
    /// surrogate pair.
    fn unicode_escape(&mut self) -> Option<char> {
        let high = self.hex4()?;
        if !(0xD800..0xDC00).contains(&high) {
            return char::from_u32(high);
        }
        if !self.text[self.at..].starts_with("\\u") {
            return None;
        }
        self.at += 2;
        let low = self.hex4()?;
        if !(0xDC00..0xE000).contains(&low) {
            return None;
        }
        char::from_u32(0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00))
    }

    /// The four hex digits at the cursor.
    fn hex4(&mut self) -> Option<u32> {
        let digits = self.text.get(self.at..self.at + 4)?;
        self.at += 4;
        u32::from_str_radix(digits, 16).ok()
    }

    /// The number at the cursor.
    fn number(&mut self) -> Option<Value> {
        let rest = &self.text[self.at..];
        let end = rest
            .find(|c: char| !(c.is_ascii_digit() || matches!(c, '-' | '+' | '.' | 'e' | 'E')))
            .unwrap_or(rest.len());
        self.at += end;
        rest[..end].parse().ok().map(Value::Number)
    }

    /// `value`, when `word` is at the cursor.
    fn word(&mut self, word: &str, value: Value) -> Option<Value> {
        if !self.text[self.at..].starts_with(word) {
            return None;
        }
        self.at += word.len();
        Some(value)
    }

    /// The byte at the cursor.
    fn peek(&self) -> Option<u8> {
        self.text.as_bytes().get(self.at).copied()
    }

    /// Moves past `byte` when it is at the cursor, and says whether it was.
    fn eat(&mut self, byte: u8) -> bool {
        let found = self.peek() == Some(byte);
        if found {
            self.at += 1;
        }
        found
    }

    /// Moves past any white space at the cursor.
    fn skip_space(&mut self) {
        while self.peek().is_some_and(|byte| byte.is_ascii_whitespace()) {
            self.at += 1;
        }
    }
}

/// `text` as a JSON string, quotes included.
pub fn quote(text: &str) -> String {
    let mut quoted = String::with_capacity(text.len() + 2);
    quoted.push('"');
    for c in text.chars() {
        match c {
            '"' => quoted.push_str(r#"\""#),
            '\\' => quoted.push_str(r"\\"),
            '\n' => quoted.push_str(r"\n"),
            '\r' => quoted.push_str(r"\r"),
            '\t' => quoted.push_str(r"\t"),
            c if c < ' ' => {
                let _ = write!(quoted, r"\u{:04x}", u32::from(c));
            }
            c => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted
}

#[cfg(test)]
mod tests {
    use super::{Value, quote};

    #[test]
    fn parses_a_cargo_message() {
        let value = Value::parse(
            r#"{"reason":"compiler-message","message":{"code":{"code":"x"},"spans":[{"line_start":12,"is_primary":true}],"text":"a \"b\" é\n"}}"#,
        )
        .unwrap();
        let message = value.get("message").unwrap();
        assert_eq!(
            message.get("code").and_then(|code| code.get("code")),
            Some(&Value::String("x".into()))
        );
        assert_eq!(
            message.get("spans").unwrap().items()[0]
                .get("line_start")
                .and_then(Value::as_usize),
            Some(12)
        );
        assert_eq!(
            message.get("text").and_then(Value::as_str),
            Some("a \"b\" é\n")
        );
    }

    #[test]
    fn refuses_trailing_text_and_broken_values() {
        assert_eq!(Value::parse("{} x"), None);
        assert_eq!(Value::parse(r#"{"a":}"#), None);
        assert_eq!(Value::parse(r#""\ud800""#), None);
    }

    #[test]
    fn quoted_text_parses_back() {
        let text = "tab\t \"quote\" back\\slash \u{1} é";
        assert_eq!(Value::parse(&quote(text)), Some(Value::String(text.into())));
    }
}
