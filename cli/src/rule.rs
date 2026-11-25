//! The rules `rusty-lints` checks, and which tool checks each.

use std::fmt;

use clap::ValueEnum;

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
    /// No decision taken from the text of an error, in tests too
    ErrorTextMatch,
    /// No closure bound with `let` and used once
    SingleUseClosure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Scan,
    Check,
}

impl Rule {
    pub const ALL: [Rule; 6] = [
        Rule::FieldComment,
        Rule::UseInFunction,
        Rule::CodeInModRs,
        Rule::VersionNumber,
        Rule::ErrorTextMatch,
        Rule::SingleUseClosure,
    ];

    /// The rule's name, as written on the command line, in `rusty-lints.toml` and in Dylint's
    /// diagnostics.
    pub fn name(self) -> &'static str {
        match self {
            Rule::FieldComment => "field_comment",
            Rule::UseInFunction => "use_in_function",
            Rule::CodeInModRs => "code_in_mod_rs",
            Rule::VersionNumber => "version_number",
            Rule::ErrorTextMatch => "error_text_match",
            Rule::SingleUseClosure => "single_use_closure",
        }
    }

    /// The rule named `name`, if there is one.
    pub fn from_name(name: &str) -> Option<Rule> {
        Rule::ALL.into_iter().find(|rule| rule.name() == name)
    }

    /// The command that checks the rule: `scan` reads the syntax, `check` runs the Dylint lints.
    pub fn tool(self) -> Tool {
        match self {
            Rule::FieldComment | Rule::UseInFunction | Rule::CodeInModRs | Rule::VersionNumber => {
                Tool::Scan
            }
            Rule::ErrorTextMatch | Rule::SingleUseClosure => Tool::Check,
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
            Rule::ErrorTextMatch => {
                "match on the error's type or variant, or downcast to it; a test asserts the type or the variant"
            }
            Rule::SingleUseClosure => {
                "write the code where it is used, or make it a function if it deserves a name"
            }
        }
    }
}

impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}
