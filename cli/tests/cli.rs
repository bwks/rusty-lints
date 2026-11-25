//! Runs `rusty-lints` on the fixtures, each copied into a git repository of its own.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::{env, fs, process};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

struct Repository {
    root: PathBuf,
}

impl Repository {
    /// A new git repository named after `test` that tracks `files`, each copied from the
    /// fixtures to the same path.
    fn new(test: &str, files: &[&str]) -> Self {
        let root = env::temp_dir().join(format!("rusty-lints-{}-{test}", process::id()));
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

    /// Writes `text` to `path`, relative to the root, and has git track it.
    fn track(&self, path: &str, text: &str) {
        let target = self.root.join(path);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, text).unwrap();
        git(&self.root, &["add", path]);
    }

    /// Runs `rusty-lints scan` on the repository with `args`.
    fn scan(&self, args: &[&str]) -> Output {
        self.run(&[&["scan"], args].concat())
    }

    /// Runs `rusty-lints` on the repository with `args`.
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_rusty-lints"))
            .arg("-C")
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
fn qualified_call_takes_crate_names_from_every_tracked_manifest() {
    let repository = Repository::new("qualified", &["qualified_call.rs"]);
    repository.track(
        "Cargo.toml",
        "[workspace.dependencies]\ntime-source = \"1\"\n",
    );
    repository.track("crates/a/Cargo.toml", "[dependencies]\nid-gen = \"1\"\n");
    let output = repository.scan(&["--rule", "qualified_call"]);
    assert_eq!(
        places(&output),
        expected("qualified_call.rs", &[4, 5, 6, 7])
    );
}

#[test]
fn qualified_call_min_segments_and_allow_come_from_the_config() {
    let repository = Repository::new("qualified-config", &["qualified_call.rs"]);
    repository.config(
        r#"
        [qualified_call]
        min_segments = 4
        allow = ["std::sync"]
        "#,
    );
    let output = repository.scan(&["--rule", "qualified_call"]);
    assert_eq!(places(&output), expected("qualified_call.rs", &[6]));
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

#[test]
fn json_format_holds_every_hit() {
    let repository = Repository::new("json", &["use_in_function.rs"]);
    let output = repository.scan(&["--rule", "use_in_function", "--format", "json"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with(r#"{"hits":[{"file":"use_in_function.rs","line":18,"#));
    assert_eq!(stdout.matches(r#""rule":"use_in_function""#).count(), 5);
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn scan_refuses_a_rule_that_check_runs() {
    let repository = Repository::new("wrong-command", &["use_in_function.rs"]);
    let output = repository.scan(&["--rule", "error_text_match"]);
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn all_with_only_scan_rules_runs_no_check() {
    let repository = Repository::new("all", &["use_in_function.rs"]);
    let output = repository.run(&["all", "--rule", "use_in_function"]);
    assert_eq!(places(&output).len(), 5);
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn init_writes_a_config_once() {
    let repository = Repository::new("init", &["use_in_function.rs"]);
    let first = repository.run(&["init"]);
    assert_eq!(first.status.code(), Some(0));
    let written = fs::read_to_string(repository.root.join("rusty-lints.toml")).unwrap();
    assert!(written.contains("[error_text_match]"));
    fs::write(repository.root.join("rusty-lints.toml"), "# mine\n").unwrap();
    let second = repository.run(&["init"]);
    assert_eq!(second.status.code(), Some(2));
    assert_eq!(
        fs::read_to_string(repository.root.join("rusty-lints.toml")).unwrap(),
        "# mine\n"
    );
}
