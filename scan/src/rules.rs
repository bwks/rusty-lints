//! The rules `rusty-scan` checks, and what each reports.

mod code_in_mod_rs;
mod field_comment;
mod use_in_function;
mod version_number;

use std::fmt;

use clap::ValueEnum;

use crate::config::Config;
use crate::source::Source;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, ValueEnum)]
#[value(rename_all = "snake_case")]
pub enum Rule {
    /// No comments on struct fields, enum variants or enum struct-variant fields
    FieldComment,
    /// No `use` declarations inside a function body or any other block
    UseInFunction,
    /// A `mod.rs` holds only `mod` declarations and `pub use` re-exports
    CodeInModRs,
    /// No version number after "since", "from", "as of", "before", "after", "until" or "in"
    VersionNumber,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Hit {
    pub path: String,
    pub line: usize,
    pub rule: Rule,
    pub text: String,
}

impl Rule {
    pub const ALL: [Rule; 4] = [
        Rule::FieldComment,
        Rule::UseInFunction,
        Rule::CodeInModRs,
        Rule::VersionNumber,
    ];

    /// The rule's name, as written on the command line and in `rusty-lints.toml`.
    pub fn name(self) -> &'static str {
        match self {
            Rule::FieldComment => "field_comment",
            Rule::UseInFunction => "use_in_function",
            Rule::CodeInModRs => "code_in_mod_rs",
            Rule::VersionNumber => "version_number",
        }
    }

    /// What to do about a hit.
    pub fn help(self) -> &'static str {
        match self {
            Rule::FieldComment => {
                "name the field or the variant for what it holds, and say the rest in the type's doc comment"
            }
            Rule::UseInFunction => {
                "declare it with the other `use` declarations at the top of the file"
            }
            Rule::CodeInModRs => {
                "a `mod.rs` holds only docs, `mod` declarations and `pub use` re-exports; move the code to a module of its own"
            }
            Rule::VersionNumber => {
                "describe what the code does now; history belongs in the changelog"
            }
        }
    }

    /// Whether the rule needs the syntax tree, rather than the tokens alone.
    fn needs_syntax(self) -> bool {
        !matches!(self, Rule::VersionNumber)
    }

    /// The 1-based lines of `source` that break the rule. `file` is the syntax tree, when the
    /// file parsed.
    fn lines(self, source: &Source, file: Option<&syn::File>, config: &Config) -> Vec<usize> {
        match (self, file) {
            (Rule::FieldComment, Some(file)) => {
                field_comment::lines(source, file, &config.field_comment.exempt_derives)
            }
            (Rule::UseInFunction, Some(file)) => use_in_function::lines(source, file),
            (Rule::CodeInModRs, Some(file)) => code_in_mod_rs::lines(source, file),
            (Rule::VersionNumber, _) => version_number::lines(source, &config.version_number.names),
            (_, None) => Vec::new(),
        }
    }
}

impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl fmt::Display for Hit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}: {}: {}",
            self.path, self.line, self.rule, self.text
        )
    }
}

/// Checks `source` against each of `rules` that does not exclude its path. Returns the hits, one
/// per rule and line, and the parse error when a rule needs the syntax tree and the file does not
/// parse; the rules that read tokens alone still run on such a file.
pub fn check(source: &Source, rules: &[Rule], config: &Config) -> (Vec<Hit>, Option<syn::Error>) {
    let rules: Vec<Rule> = rules
        .iter()
        .copied()
        .filter(|&rule| !config.excluded(rule, &source.path))
        .collect();
    let (file, error) = if rules.iter().any(|rule| rule.needs_syntax()) {
        match source.parse() {
            Ok(file) => (Some(file), None),
            Err(error) => (None, Some(error)),
        }
    } else {
        (None, None)
    };
    let mut hits = Vec::new();
    for rule in rules {
        let mut lines = rule.lines(source, file.as_ref(), config);
        lines.sort_unstable();
        lines.dedup();
        hits.extend(lines.into_iter().map(|line| Hit {
            path: source.path.clone(),
            line,
            rule,
            text: source.line_text(line).to_owned(),
        }));
    }
    (hits, error)
}
