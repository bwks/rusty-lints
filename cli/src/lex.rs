//! A lexer that splits Rust source into the tokens the rules read: comments, string literals,
//! identifiers and punctuation. `syn` drops plain comments, so the rules that look at comments
//! read the text through this lexer.

use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Comment,
    Str,
    Char,
    Ident,
    Number,
    Lifetime,
    Punct(u8),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: Kind,
    pub range: Range<usize>,
}

/// Splits `source` into tokens, skipping whitespace. Text the lexer cannot close, such as an
/// unterminated string, runs to the end of the source.
pub fn tokens(source: &str) -> Vec<Token> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let start = at;
        let byte = bytes[at];
        let kind = if byte.is_ascii_whitespace() {
            at += 1;
            continue;
        } else if bytes[at..].starts_with(b"//") {
            at = find_byte(bytes, at, b'\n');
            Kind::Comment
        } else if bytes[at..].starts_with(b"/*") {
            at = block_comment_end(bytes, at);
            Kind::Comment
        } else if byte == b'"' {
            at = string_end(bytes, at + 1);
            Kind::Str
        } else if byte == b'\'' {
            let (end, kind) = quote_end(bytes, at);
            at = end;
            kind
        } else if byte.is_ascii_digit() {
            at = number_end(bytes, at);
            Kind::Number
        } else if is_ident_byte(byte) {
            let (end, kind) = ident_or_prefixed_literal(bytes, at);
            at = end;
            kind
        } else {
            at += 1;
            Kind::Punct(byte)
        };
        tokens.push(Token {
            kind,
            range: start..at.min(bytes.len()),
        });
    }
    tokens
}

/// The index of the first `needle` at or after `from`, or the end of `bytes`.
fn find_byte(bytes: &[u8], from: usize, needle: u8) -> usize {
    bytes[from..]
        .iter()
        .position(|&byte| byte == needle)
        .map_or(bytes.len(), |offset| from + offset)
}

/// The end of the block comment that opens at `start`, following nested comments.
fn block_comment_end(bytes: &[u8], start: usize) -> usize {
    let mut depth = 0;
    let mut at = start;
    while at < bytes.len() {
        if bytes[at..].starts_with(b"/*") {
            depth += 1;
            at += 2;
        } else if bytes[at..].starts_with(b"*/") {
            depth -= 1;
            at += 2;
            if depth == 0 {
                return at;
            }
        } else {
            at += 1;
        }
    }
    bytes.len()
}

/// The end of a string whose text starts at `from`, just after its opening quote.
fn string_end(bytes: &[u8], from: usize) -> usize {
    let mut at = from;
    while at < bytes.len() {
        match bytes[at] {
            b'\\' => at += 2,
            b'"' => return at + 1,
            _ => at += 1,
        }
    }
    bytes.len()
}

/// The end of a raw string whose `#` marks start at `from`, or `None` when no `"` follows them.
fn raw_string_end(bytes: &[u8], from: usize) -> Option<usize> {
    let hashes = bytes[from..]
        .iter()
        .take_while(|&&byte| byte == b'#')
        .count();
    if bytes.get(from + hashes) != Some(&b'"') {
        return None;
    }
    let mut at = from + hashes + 1;
    while at < bytes.len() {
        if bytes[at] == b'"'
            && bytes[at + 1..]
                .iter()
                .take(hashes)
                .filter(|&&byte| byte == b'#')
                .count()
                == hashes
        {
            return Some(at + 1 + hashes);
        }
        at += 1;
    }
    Some(bytes.len())
}

/// The end and kind of a token opening with `'` at `start`: a char literal or a lifetime.
fn quote_end(bytes: &[u8], start: usize) -> (usize, Kind) {
    match bytes.get(start + 1) {
        Some(b'\\') => {
            let mut at = start + 1;
            while at < bytes.len() && bytes[at] != b'\'' && bytes[at] != b'\n' {
                at += if bytes[at] == b'\\' { 2 } else { 1 };
            }
            ((at + 1).min(bytes.len()), Kind::Char)
        }
        Some(_) => {
            let width = char_width(bytes, start + 1);
            if bytes.get(start + 1 + width) == Some(&b'\'') {
                return (start + 2 + width, Kind::Char);
            }
            let mut at = start + 1;
            while at < bytes.len() && is_ident_byte(bytes[at]) {
                at += 1;
            }
            (at.max(start + 1), Kind::Lifetime)
        }
        None => (start + 1, Kind::Punct(b'\'')),
    }
}

/// The width in bytes of the UTF-8 character starting at `at`.
fn char_width(bytes: &[u8], at: usize) -> usize {
    match bytes.get(at) {
        Some(byte) if *byte >= 0xF0 => 4,
        Some(byte) if *byte >= 0xE0 => 3,
        Some(byte) if *byte >= 0xC0 => 2,
        _ => 1,
    }
}

/// The end of the number literal that starts at `start`.
fn number_end(bytes: &[u8], start: usize) -> usize {
    let mut at = start;
    while at < bytes.len() {
        let byte = bytes[at];
        let decimal_point = byte == b'.' && bytes.get(at + 1).is_some_and(u8::is_ascii_digit);
        if byte.is_ascii_alphanumeric() || byte == b'_' || decimal_point {
            at += 1;
        } else {
            break;
        }
    }
    at
}

/// The end and kind of an identifier at `start`, or of a literal it prefixes: `b"..."`,
/// `c"..."`, `r"..."`, `br#"..."#`, `cr"..."`, `b'x'`.
fn ident_or_prefixed_literal(bytes: &[u8], start: usize) -> (usize, Kind) {
    let mut at = start;
    while at < bytes.len() && is_ident_byte(bytes[at]) {
        at += 1;
    }
    let prefix = &bytes[start..at];
    match (prefix, bytes.get(at)) {
        (b"b" | b"c", Some(b'"')) => (string_end(bytes, at + 1), Kind::Str),
        (b"b", Some(b'\'')) => (quote_end(bytes, at).0, Kind::Char),
        (b"r" | b"br" | b"cr", Some(b'"' | b'#')) => {
            if let Some(end) = raw_string_end(bytes, at) {
                return (end, Kind::Str);
            }
            let mut end = at + 1;
            while end < bytes.len() && is_ident_byte(bytes[end]) {
                end += 1;
            }
            (end, Kind::Ident)
        }
        _ => (at, Kind::Ident),
    }
}

/// Whether `byte` can be part of an identifier. Bytes of non-ASCII characters count, as they
/// appear in code only within identifiers.
fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte >= 0x80
}

#[cfg(test)]
mod tests {
    use super::{Kind, tokens};

    /// The kinds and texts of the tokens of `source`.
    fn lexed(source: &str) -> Vec<(Kind, &str)> {
        tokens(source)
            .into_iter()
            .map(|token| (token.kind, &source[token.range]))
            .collect()
    }

    #[test]
    fn comments_end_at_line_end_or_matching_close() {
        let source = "a // b\n/* c /* d */ e */ f";
        assert_eq!(
            lexed(source),
            vec![
                (Kind::Ident, "a"),
                (Kind::Comment, "// b"),
                (Kind::Comment, "/* c /* d */ e */"),
                (Kind::Ident, "f"),
            ]
        );
    }

    #[test]
    fn comment_markers_inside_strings_are_text() {
        let source = r##"x("a // b", r#"c /* "d" */"#, b"\"//", c"e")"##;
        let strings: Vec<_> = lexed(source)
            .into_iter()
            .filter(|(kind, _)| *kind == Kind::Str)
            .map(|(_, text)| text)
            .collect();
        assert_eq!(
            strings,
            vec![
                r#""a // b""#,
                r##"r#"c /* "d" */"#"##,
                r#"b"\"//""#,
                r#"c"e""#
            ]
        );
    }

    #[test]
    fn char_literals_and_lifetimes_differ() {
        let source = "fn f<'a>(x: &'a u8) { let q = '\"'; let b = '{'; let e = '\\''; }";
        let found: Vec<_> = lexed(source)
            .into_iter()
            .filter(|(kind, _)| matches!(kind, Kind::Char | Kind::Lifetime))
            .collect();
        assert_eq!(
            found,
            vec![
                (Kind::Lifetime, "'a"),
                (Kind::Lifetime, "'a"),
                (Kind::Char, "'\"'"),
                (Kind::Char, "'{'"),
                (Kind::Char, "'\\''"),
            ]
        );
    }

    #[test]
    fn raw_identifier_is_not_a_raw_string() {
        assert_eq!(
            lexed("r#match + 1.5"),
            vec![
                (Kind::Ident, "r#match"),
                (Kind::Punct(b'+'), "+"),
                (Kind::Number, "1.5"),
            ]
        );
    }
}
