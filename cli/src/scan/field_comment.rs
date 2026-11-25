//! `field_comment`: no comments on struct fields, enum variants or the fields of an enum's
//! struct variants, except in types whose derives include one of the exempt derives.
//!
//! Every comment inside the body of a struct or an enum is a hit, on each line it spans, as is a
//! `#[doc = ...]` attribute. Types nested in functions count; types that only a macro's input
//! holds are not seen.

use std::ops::Range;

use proc_macro2::{Delimiter, TokenStream, TokenTree};
use syn::visit::{self, Visit};
use syn::{Attribute, Fields, ItemEnum, ItemStruct, Meta};

use crate::lex::Kind;
use crate::source::Source;

struct Bodies<'a> {
    exempt_derives: &'a [String],
    ranges: Vec<Range<usize>>,
}

impl<'ast> Visit<'ast> for Bodies<'_> {
    fn visit_item_struct(&mut self, item: &'ast ItemStruct) {
        if !self.exempt(&item.attrs) {
            let span = match &item.fields {
                Fields::Named(fields) => Some(fields.brace_token.span.join()),
                Fields::Unnamed(fields) => Some(fields.paren_token.span.join()),
                Fields::Unit => None,
            };
            self.ranges.extend(span.map(|span| span.byte_range()));
        }
        visit::visit_item_struct(self, item);
    }

    fn visit_item_enum(&mut self, item: &'ast ItemEnum) {
        if !self.exempt(&item.attrs) {
            self.ranges.push(item.brace_token.span.join().byte_range());
        }
        visit::visit_item_enum(self, item);
    }
}

impl Bodies<'_> {
    /// Whether the attributes derive one of the exempt derives, directly or inside a
    /// `cfg_attr`.
    fn exempt(&self, attrs: &[Attribute]) -> bool {
        attrs.iter().any(|attr| {
            let Meta::List(list) = &attr.meta else {
                return false;
            };
            let mut derives = Vec::new();
            if list.path.is_ident("derive") {
                derive_names(list.tokens.clone(), &mut derives);
            } else {
                nested_derives(list.tokens.clone(), &mut derives);
            }
            derives
                .iter()
                .any(|name| self.exempt_derives.iter().any(|exempt| exempt == name))
        })
    }
}

/// The lines of `source` with a comment on a field or a variant.
pub fn lines(source: &Source, file: &syn::File, exempt_derives: &[String]) -> Vec<usize> {
    let mut bodies = Bodies {
        exempt_derives,
        ranges: Vec::new(),
    };
    bodies.visit_file(file);
    let mut lines = Vec::new();
    for body in bodies.ranges {
        let tokens: Vec<_> = source
            .tokens
            .iter()
            .filter(|token| token.range.start > body.start && token.range.end < body.end)
            .collect();
        for (at, token) in tokens.iter().enumerate() {
            match token.kind {
                Kind::Comment => lines.extend(
                    source.line_of(token.range.start)..=source.line_of(token.range.end - 1),
                ),
                Kind::Punct(b'#') => {
                    let doc_attribute = tokens.get(at + 1..at + 4).is_some_and(|next| {
                        next[0].kind == Kind::Punct(b'[')
                            && next[1].kind == Kind::Ident
                            && &source.text[next[1].range.clone()] == "doc"
                            && next[2].kind == Kind::Punct(b'=')
                    });
                    if doc_attribute {
                        lines.push(source.line_of(token.range.start));
                    }
                }
                _ => {}
            }
        }
    }
    lines
}

/// Adds the last segment of each path in the list of a `derive(...)` to `names`.
fn derive_names(tokens: TokenStream, names: &mut Vec<String>) {
    let mut last = None;
    for tree in tokens {
        match tree {
            TokenTree::Ident(ident) => last = Some(ident.to_string()),
            TokenTree::Punct(punct) if punct.as_char() == ',' => names.extend(last.take()),
            _ => {}
        }
    }
    names.extend(last);
}

/// Adds the names of every `derive(...)` nested in `tokens`, as in a `cfg_attr`, to `names`.
fn nested_derives(tokens: TokenStream, names: &mut Vec<String>) {
    let mut after_derive = false;
    for tree in tokens {
        match tree {
            TokenTree::Ident(ident) => {
                after_derive = ident == "derive";
                continue;
            }
            TokenTree::Group(group) if group.delimiter() == Delimiter::Parenthesis => {
                if after_derive {
                    derive_names(group.stream(), names);
                } else {
                    nested_derives(group.stream(), names);
                }
            }
            _ => {}
        }
        after_derive = false;
    }
}
