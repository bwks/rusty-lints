//! A Rust file read for scanning: its text, its syntax tree and its lines.

use std::iter;

use crate::lex::{self, Token};

pub struct Source {
    pub path: String,
    pub text: String,
    pub tokens: Vec<Token>,
    line_starts: Vec<usize>,
}

impl Source {
    /// Lexes `text`, read from `path` relative to the repository root.
    pub fn new(path: String, text: String) -> Self {
        let line_starts = iter::once(0)
            .chain(text.match_indices('\n').map(|(at, _)| at + 1))
            .collect();
        let tokens = lex::tokens(&text);
        Self {
            path,
            text,
            tokens,
            line_starts,
        }
    }

    /// Parses the file with `syn`. The byte offsets of the tree's spans are offsets into
    /// `self.text`: a byte-order mark and a shebang line, which `syn` would cut off, are blanked
    /// out instead.
    pub fn parse(&self) -> syn::Result<syn::File> {
        let mut text = self.text.clone();
        if let Some(rest) = text.strip_prefix('\u{feff}') {
            text = format!("   {rest}");
        }
        let trimmed = text.trim_start();
        if trimmed.starts_with("#!") && !trimmed[2..].trim_start().starts_with('[') {
            let at = text.len() - trimmed.len();
            text.replace_range(at..at + 2, "//");
        }
        syn::parse_file(&text)
    }

    /// The 1-based line that holds byte `offset`.
    pub fn line_of(&self, offset: usize) -> usize {
        self.line_starts.partition_point(|&start| start <= offset)
    }

    /// The text of 1-based `line` without its indentation and line ending.
    pub fn line_text(&self, line: usize) -> &str {
        let start = self.line_starts[line - 1];
        let end = self
            .line_starts
            .get(line)
            .map_or(self.text.len(), |&next| next - 1);
        self.text[start..end].trim()
    }
}

#[cfg(test)]
mod tests {
    use syn::Item;

    use super::Source;

    #[test]
    fn offsets_map_to_lines() {
        let source = Source::new("a.rs".into(), "fn a() {}\n  let b;\r\n\nc".into());
        assert_eq!(source.line_of(0), 1);
        assert_eq!(source.line_of(9), 1);
        assert_eq!(source.line_of(10), 2);
        assert_eq!(source.line_text(2), "let b;");
        assert_eq!(source.line_text(4), "c");
    }

    #[test]
    fn shebang_keeps_offsets() {
        let source = Source::new("a.rs".into(), "#!/usr/bin/env run\nstruct A;\n".into());
        let file = source.parse().unwrap();
        let Item::Struct(item) = &file.items[0] else {
            panic!("expected a struct");
        };
        assert_eq!(item.ident.span().byte_range(), 26..27);
    }
}
