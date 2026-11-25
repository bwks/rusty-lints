//! What a run found, how it is printed and the exit code it calls for.

use std::collections::BTreeMap;
use std::fmt;
use std::process::ExitCode;

use clap::ValueEnum;

use crate::json;
use crate::rule::Rule;

const EXIT_HITS: u8 = 1;
pub const EXIT_FAILURE: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Format {
    /// `file:line: rule: text`, one hit per line, and a count per rule on stderr
    Text,
    /// One JSON object: `{"hits": [...], "failures": [...]}`
    Json,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Hit {
    pub path: String,
    pub line: usize,
    pub column: Option<usize>,
    pub rule: Rule,
    pub text: String,
    pub message: String,
}

#[derive(Debug, Default)]
pub struct Report {
    pub hits: Vec<Hit>,
    pub failures: Vec<String>,
}

impl Report {
    /// Adds the hits and the failures of `other`.
    pub fn extend(&mut self, other: Report) {
        self.hits.extend(other.hits);
        self.failures.extend(other.failures);
    }

    /// Prints the report in `format` and returns the exit code it calls for: 2 when a tool
    /// failed, 1 when there is a hit, 0 otherwise.
    pub fn print(mut self, format: Format) -> ExitCode {
        self.hits.sort();
        self.hits.dedup();
        for failure in &self.failures {
            eprintln!("rusty-lints: {failure}");
        }
        match format {
            Format::Text => self.print_text(),
            Format::Json => println!("{}", self.to_json()),
        }
        if !self.failures.is_empty() {
            ExitCode::from(EXIT_FAILURE)
        } else if self.hits.is_empty() {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(EXIT_HITS)
        }
    }

    /// Prints each hit on stdout, then a count and a hint per rule on stderr.
    fn print_text(&self) {
        for hit in &self.hits {
            println!("{hit}");
        }
        let mut counts: BTreeMap<Rule, usize> = BTreeMap::new();
        for hit in &self.hits {
            *counts.entry(hit.rule).or_default() += 1;
        }
        for (rule, count) in &counts {
            eprintln!("rusty-lints: {rule}: {count} hits; {}", rule.help());
        }
    }

    /// The report as one JSON object.
    fn to_json(&self) -> String {
        let hits: Vec<String> = self
            .hits
            .iter()
            .map(|hit| {
                format!(
                    r#"{{"file":{},"line":{},"column":{},"rule":{},"text":{},"message":{}}}"#,
                    json::quote(&hit.path),
                    hit.line,
                    hit.column
                        .map_or_else(|| "null".to_owned(), |column| column.to_string()),
                    json::quote(hit.rule.name()),
                    json::quote(&hit.text),
                    json::quote(&hit.message),
                )
            })
            .collect();
        let failures: Vec<String> = self
            .failures
            .iter()
            .map(|failure| json::quote(failure))
            .collect();
        format!(
            r#"{{"hits":[{}],"failures":[{}]}}"#,
            hits.join(","),
            failures.join(",")
        )
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

#[cfg(test)]
mod tests {
    use super::{Hit, Report};
    use crate::json::Value;
    use crate::rule::Rule;

    #[test]
    fn json_report_holds_each_hit_and_failure() {
        let report = Report {
            hits: vec![Hit {
                path: "src/a.rs".into(),
                line: 3,
                column: Some(5),
                rule: Rule::ErrorTextMatch,
                text: r#"assert!(e.to_string().contains("x"));"#.into(),
                message: "test checks an error by its text".into(),
            }],
            failures: vec!["it broke".into()],
        };
        let value = Value::parse(&report.to_json()).unwrap();
        let hit = &value.get("hits").unwrap().items()[0];
        assert_eq!(hit.get("file").and_then(Value::as_str), Some("src/a.rs"));
        assert_eq!(hit.get("line").and_then(Value::as_usize), Some(3));
        assert_eq!(hit.get("column").and_then(Value::as_usize), Some(5));
        assert_eq!(
            hit.get("rule").and_then(Value::as_str),
            Some("error_text_match")
        );
        assert_eq!(
            hit.get("text").and_then(Value::as_str),
            Some(r#"assert!(e.to_string().contains("x"));"#)
        );
        assert_eq!(
            value.get("failures").unwrap().items(),
            [Value::String("it broke".into())]
        );
    }
}
