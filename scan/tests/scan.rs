//! Runs `rusty-scan` on the fixtures, each copied into a git repository of its own.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

struct Repository {
    root: PathBuf,
}

impl Repository {
    /// A new git repository named after `test` that tracks `files`, each copied from the
    /// fixtures to the same path.
    fn new(test: &str, files: &[&str]) -> Self {
        let root = std::env::temp_dir().join(format!("rusty-scan-{}-{test}", std::process::id()));
        if root.exists() {
            fs::remove_dir_all(&root).unwrap();
        }
        fs::create_dir_all(&root).unwrap();
        for file in files {
            let target = root.join(file);
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::copy(Path::new(FIXTURES).join(file), target).unwrap();
        }
        git(&root, &["init", "-q"]);
        git(&root, &["add", "."]);
        Self { root }
    }

    /// Writes `text` to `rusty-lints.toml` at the root.
    fn config(&self, text: &str) {
        fs::write(self.root.join("rusty-lints.toml"), text).unwrap();
    }

    /// Runs `rusty-scan` on the repository with `args`.
    fn scan(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_rusty-scan"))
            .arg(&self.root)
            .args(args)
            .output()
            .unwrap()
    }
}

impl Drop for Repository {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Runs git in `dir`, failing the test when it fails.
fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

/// The `file:line` of each hit the scan printed.
fn places(output: &Output) -> Vec<String> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| line.splitn(3, ':').take(2).collect::<Vec<_>>().join(":"))
        .collect()
}

/// `file:line` for each of `lines` in `file`.
fn expected(file: &str, lines: &[usize]) -> Vec<String> {
    lines.iter().map(|line| format!("{file}:{line}")).collect()
}

#[test]
fn field_comment_finds_the_lines_of_the_awk_self_test() {
    let repository = Repository::new("awk", &["field_comment.rs"]);
    let output = repository.scan(&["--rule", "field_comment"]);
    assert_eq!(
        places(&output),
        expected(
            "field_comment.rs",
            &[6, 8, 10, 11, 13, 19, 27, 29, 34, 36, 37, 39, 71]
        )
    );
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn field_comment_output_names_file_line_rule_and_text() {
    let repository = Repository::new("format", &["field_comment.rs"]);
    let output = repository.scan(&["--rule", "field_comment"]);
    let first = String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .map(str::to_owned);
    assert_eq!(
        first.as_deref(),
        Some("field_comment.rs:6: field_comment: /// Field doc.")
    );
}

#[test]
fn field_comment_reads_derives_and_cfg_code() {
    let repository = Repository::new("derives", &["field_comment_derives.rs"]);
    let output = repository.scan(&["--rule", "field_comment"]);
    assert_eq!(
        places(&output),
        expected("field_comment_derives.rs", &[18, 39])
    );
}

#[test]
fn field_comment_exempt_derives_come_from_the_config() {
    let repository = Repository::new("exempt", &["field_comment_derives.rs"]);
    repository.config(
        r#"
        [field_comment]
        exempt_derives = ["ParserLike"]
        "#,
    );
    let output = repository.scan(&["--rule", "field_comment"]);
    assert_eq!(
        places(&output),
        expected("field_comment_derives.rs", &[5, 12, 39])
    );
}

#[test]
fn use_in_function_finds_uses_in_blocks_only() {
    let repository = Repository::new("use", &["use_in_function.rs"]);
    let output = repository.scan(&["--rule", "use_in_function"]);
    assert_eq!(
        places(&output),
        expected("use_in_function.rs", &[18, 26, 33, 42, 52])
    );
}

#[test]
fn code_in_mod_rs_finds_items_other_than_mod_and_pub_use() {
    let repository = Repository::new(
        "mod",
        &[
            "code_in_mod_rs/clean/mod.rs",
            "code_in_mod_rs/with_code/mod.rs",
        ],
    );
    let output = repository.scan(&["--rule", "code_in_mod_rs"]);
    assert_eq!(
        places(&output),
        expected("code_in_mod_rs/with_code/mod.rs", &[3, 9, 11, 13, 17])
    );
}

#[test]
fn version_number_finds_numbers_in_comments_and_strings() {
    let repository = Repository::new("version", &["version_number.rs"]);
    let output = repository.scan(&["--rule", "version_number"]);
    assert_eq!(
        places(&output),
        expected("version_number.rs", &[1, 3, 6, 7, 8])
    );
}

#[test]
fn disabled_rule_and_excluded_path_report_nothing() {
    let repository = Repository::new("config", &["version_number.rs", "use_in_function.rs"]);
    repository.config(
        r#"
        exclude = ["use_in_function.rs"]
        [version_number]
        enabled = false
        "#,
    );
    let output = repository.scan(&[]);
    assert_eq!(places(&output), Vec::<String>::new());
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn file_that_does_not_parse_fails_the_scan() {
    let repository = Repository::new("broken", &["use_in_function.rs"]);
    fs::write(repository.root.join("use_in_function.rs"), "fn broken( {").unwrap();
    let output = repository.scan(&["--rule", "use_in_function"]);
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn unknown_config_key_fails_the_scan() {
    let repository = Repository::new("bad-config", &["use_in_function.rs"]);
    repository.config("[use_in_function]\nenable = false\n");
    let output = repository.scan(&[]);
    assert_eq!(output.status.code(), Some(2));
}
