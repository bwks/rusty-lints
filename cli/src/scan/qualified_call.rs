//! `qualified_call`: an expression never names an item through a long path that starts with a
//! crate, as in `jiff::Timestamp::now()`. The item is imported with `use` at the top of the file.
//!
//! A path counts in an expression (a call, a reference to a function or a constant), a struct
//! literal and a pattern. Types, `use` declarations and attributes are not checked. A crate is a
//! dependency named in a `Cargo.toml` git tracks, a package of the repository, `std`, `core` or
//! `alloc`; a name the file binds itself, with `mod` or `use`, is not a crate in that file. A
//! macro's input is checked when it parses as a list of expressions separated by commas, as in
//! `format!` and `assert_eq!`.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::visit::{self, Visit};
use syn::{
    Attribute, Expr, ExprPath, ExprStruct, ItemMod, Macro, PatStruct, PatTupleStruct, QSelf, Token,
    UseTree,
};
use toml::{Table, Value};

use crate::config::{Config, QualifiedCall};
use crate::repository;
use crate::rule::Rule;
use crate::source::Source;

const LANGUAGE_CRATES: [&str; 3] = ["std", "core", "alloc"];

const DEPENDENCY_TABLES: [&str; 3] = ["dependencies", "dev-dependencies", "build-dependencies"];

struct Paths<'a> {
    crates: &'a BTreeSet<String>,
    settings: &'a QualifiedCall,
    starts: Vec<usize>,
}

struct Bindings {
    names: BTreeSet<String>,
}

impl<'ast> Visit<'ast> for Paths<'_> {
    fn visit_attribute(&mut self, _: &'ast Attribute) {}

    fn visit_expr_path(&mut self, expr: &'ast ExprPath) {
        self.check(expr.qself.as_ref(), &expr.path);
        visit::visit_expr_path(self, expr);
    }

    fn visit_expr_struct(&mut self, expr: &'ast ExprStruct) {
        self.check(expr.qself.as_ref(), &expr.path);
        visit::visit_expr_struct(self, expr);
    }

    fn visit_pat_struct(&mut self, pat: &'ast PatStruct) {
        self.check(pat.qself.as_ref(), &pat.path);
        visit::visit_pat_struct(self, pat);
    }

    fn visit_pat_tuple_struct(&mut self, pat: &'ast PatTupleStruct) {
        self.check(pat.qself.as_ref(), &pat.path);
        visit::visit_pat_tuple_struct(self, pat);
    }

    fn visit_macro(&mut self, mac: &'ast Macro) {
        if let Ok(exprs) =
            Punctuated::<Expr, Token![,]>::parse_terminated.parse2(mac.tokens.clone())
        {
            for expr in &exprs {
                self.visit_expr(expr);
            }
        }
    }
}

impl Paths<'_> {
    /// Records where `path` starts when it is long enough, starts with a crate and no allowed
    /// prefix covers it. A path behind a qualified self type, `<T as Trait>::f`, is not checked.
    fn check(&mut self, qself: Option<&QSelf>, path: &syn::Path) {
        if qself.is_some() || path.segments.len() < self.settings.min_segments {
            return;
        }
        let names: Vec<String> = path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect();
        if path.leading_colon.is_none() && !self.crates.contains(&names[0]) {
            return;
        }
        let allowed = self.settings.allow.iter().any(|prefix| {
            let prefix: Vec<&str> = prefix.split("::").collect();
            names.len() >= prefix.len()
                && names.iter().zip(&prefix).all(|(name, want)| name == want)
        });
        if !allowed {
            let start = path
                .leading_colon
                .map_or_else(|| path.segments[0].ident.span(), |colon| colon.spans[0]);
            self.starts.push(start.byte_range().start);
        }
    }
}

impl<'ast> Visit<'ast> for Bindings {
    fn visit_item_mod(&mut self, item: &'ast ItemMod) {
        self.names.insert(item.ident.to_string());
        visit::visit_item_mod(self, item);
    }

    fn visit_use_tree(&mut self, tree: &'ast UseTree) {
        match tree {
            UseTree::Name(name) if name.ident != "self" => {
                self.names.insert(name.ident.to_string());
            }
            UseTree::Rename(rename) => {
                self.names.insert(rename.rename.to_string());
            }
            _ => {}
        }
        visit::visit_use_tree(self, tree);
    }
}

/// The lines of `source` with an expression or pattern path that names an item through a crate.
/// `crates` are the crate names of the repository.
pub fn lines(
    source: &Source,
    file: &syn::File,
    crates: &BTreeSet<String>,
    settings: &QualifiedCall,
) -> Vec<usize> {
    let mut bindings = Bindings {
        names: BTreeSet::new(),
    };
    bindings.visit_file(file);
    let crates: BTreeSet<String> = crates.difference(&bindings.names).cloned().collect();
    let mut paths = Paths {
        crates: &crates,
        settings,
        starts: Vec::new(),
    };
    paths.visit_file(file);
    paths
        .starts
        .into_iter()
        .map(|start| source.line_of(start))
        .collect()
}

/// The crate names the code of the repository at `root` can start a path with: `std`, `core`,
/// `alloc`, and each dependency and package named in a `Cargo.toml` git tracks that
/// `qualified_call` does not exclude, with `-` written as `_`.
pub fn crate_names(root: &Path, config: &Config) -> Result<BTreeSet<String>> {
    let mut names: BTreeSet<String> = LANGUAGE_CRATES.map(String::from).into();
    for path in repository::tracked_files(root, &["Cargo.toml", "*/Cargo.toml"])? {
        if config.excluded(Rule::QualifiedCall, &path) {
            continue;
        }
        let full_path = root.join(&path);
        if !full_path.is_file() {
            continue;
        }
        let text = fs::read_to_string(&full_path)
            .with_context(|| format!("reading {}", full_path.display()))?;
        let manifest: Table =
            toml::from_str(&text).with_context(|| format!("parsing {}", full_path.display()))?;
        names.extend(manifest_names(&manifest));
    }
    Ok(names)
}

/// The crate names a manifest declares: the package, the library, and the keys of its
/// dependency tables, those of each `target` and those of `[workspace.dependencies]`.
fn manifest_names(manifest: &Table) -> Vec<String> {
    let mut tables: Vec<&Table> = DEPENDENCY_TABLES
        .iter()
        .filter_map(|name| manifest.get(*name).and_then(Value::as_table))
        .collect();
    if let Some(targets) = manifest.get("target").and_then(Value::as_table) {
        for target in targets.values().filter_map(Value::as_table) {
            tables.extend(
                DEPENDENCY_TABLES
                    .iter()
                    .filter_map(|name| target.get(*name).and_then(Value::as_table)),
            );
        }
    }
    if let Some(dependencies) = manifest
        .get("workspace")
        .and_then(|workspace| workspace.get("dependencies"))
        .and_then(Value::as_table)
    {
        tables.push(dependencies);
    }
    let own = ["package", "lib"].into_iter().filter_map(|section| {
        manifest
            .get(section)
            .and_then(|table| table.get("name"))
            .and_then(Value::as_str)
    });
    tables
        .into_iter()
        .flat_map(Table::keys)
        .map(String::as_str)
        .chain(own)
        .map(|name| name.replace('-', "_"))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use toml::Table;

    use super::{lines, manifest_names};
    use crate::config::QualifiedCall;
    use crate::source::Source;

    const SAMPLE: &str = r#"use jiff::civil::Date;
use std::time::Instant;
mod serde_json;

#[derive(Debug)]
#[jiff::civil::attribute]
struct S {
    at: jiff::civil::Date,
}

fn flagged(value: Value) -> u64 {
    let now = jiff::Timestamp::now();
    let start = std::time::Instant::now();
    let id = uuid::Uuid::new_v4();
    let max = core::u64::MAX;
    let from = std::convert::identity;
    let point = geo_types::point::Point { x: 1, y: 2 };
    let leading = ::jiff::Timestamp::now();
    match value {
        uuid::Kind::Nil => {}
        geo_types::point::Pair(a, b) => {}
        geo_types::point::Named { a, .. } => {}
    }
    println!("{}", jiff::Timestamp::now());
    let generic = alloc::vec::Vec::<u8>::new();
    0
}

fn not_flagged() {
    let local = crate::time::now();
    let here = self::time::now();
    let parent = super::time::now();
    let own = Self::make::now();
    let short = jiff::now();
    let module = serde_json::value::Value::Null;
    let unknown = mymod::Thing::new();
    let typed: jiff::civil::Date = Default::default();
    let cast = Vec::<std::time::Duration>::new();
    let qualified = <Thing as std::default::Default>::default();
    let allowed = std::sync::atomic::Ordering::SeqCst;
    let opaque = json!({ "a": std::time::Instant::now() });
}
"#;

    const FLAGGED_LINES: [usize; 12] = [12, 13, 14, 15, 16, 17, 18, 20, 21, 22, 24, 25];

    #[test]
    fn flags_long_crate_paths_in_expressions_and_patterns_only() {
        let source = Source::new("a.rs".into(), SAMPLE.into());
        let file = source.parse().unwrap();
        let crates: BTreeSet<String> = [
            "std",
            "core",
            "alloc",
            "jiff",
            "uuid",
            "geo_types",
            "serde_json",
        ]
        .map(String::from)
        .into();
        let settings = QualifiedCall {
            allow: vec!["std::sync::atomic".into()],
            ..QualifiedCall::default()
        };
        let mut found = lines(&source, &file, &crates, &settings);
        found.sort_unstable();
        found.dedup();
        assert_eq!(found, FLAGGED_LINES);
    }

    #[test]
    fn min_segments_raises_the_length_that_counts() {
        let source = Source::new("a.rs".into(), SAMPLE.into());
        let file = source.parse().unwrap();
        let crates: BTreeSet<String> = ["std", "geo_types"].map(String::from).into();
        let settings = QualifiedCall {
            min_segments: 5,
            ..QualifiedCall::default()
        };
        assert_eq!(lines(&source, &file, &crates, &settings), [40]);
    }

    #[test]
    fn manifest_names_read_every_dependency_table() {
        let manifest: Table = toml::from_str(
            r#"
            [package]
            name = "my-tool"
            [lib]
            name = "my_lib"
            [dependencies]
            serde-json = "1"
            [dev-dependencies]
            tempfile = "3"
            [build-dependencies]
            cc = "1"
            [target.'cfg(unix)'.dependencies]
            nix = "0.30"
            [workspace.dependencies]
            jiff = "0.2"
            "#,
        )
        .unwrap();
        let names: BTreeSet<String> = manifest_names(&manifest).into_iter().collect();
        assert_eq!(
            names,
            [
                "cc",
                "jiff",
                "my_lib",
                "my_tool",
                "nix",
                "serde_json",
                "tempfile"
            ]
            .map(String::from)
            .into()
        );
    }
}
