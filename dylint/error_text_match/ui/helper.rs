// compile-flags: --test

use std::io;

fn failing() -> Result<(), io::Error> {
    Err(io::Error::other("out of space"))
}

fn message() -> String {
    failing().unwrap_err().to_string()
}

fn maybe_message() -> Option<String> {
    failing().err().map(|error| error.to_string())
}

fn formatted() -> String {
    format!("{:#}", failing().unwrap_err())
}

fn label() -> String {
    String::from("out of space")
}

#[test]
fn text_through_a_helper() {
    assert_eq!(message(), "out of space");
    assert!(message().contains("space"));
    assert_eq!(maybe_message().as_deref(), Some("out of space"));
    assert!(formatted().starts_with("out"));
    let text = maybe_message().unwrap();
    assert!(text.ends_with("space"));
}

#[test]
fn text_that_is_not_an_error() {
    assert_eq!(label(), "out of space");
    assert_eq!(std::env::args().next().as_deref(), Some("x"));
}

fn main() {}
