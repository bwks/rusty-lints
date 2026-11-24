//! Since 0.7.0 the name is cached.

/* Before 0.6 this was a macro. */
fn messages() -> Vec<String> {
    vec![
        String::from("removed in v1.2.0"),
        format!("as of version 3.1, {}", 1),
        String::from(r#"the "old" layout, until 2.0.1"#),
    ]
}

/// The value is in 10.0.0.0/24 and was listed in 4 places.
fn not_reported() -> &'static str {
    // Wait until 2.5 seconds have passed; since1.2.3 is not a word.
    let in_1_2_3 = "within 1.2.3";
    in_1_2_3
}
