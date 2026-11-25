//! `version_number`: no version number after "since", "from", "as of", "before", "after",
//! "until" or "in" in a comment or a string literal, as in `since 0.7.0` or `before v2.1`.
//!
//! After "since", "before" and "as of" a two-part number (`0.7`) counts; after the others only a
//! three-part one does, as a two-part number there is more often a quantity (`until 2.5
//! seconds`). The number may follow "version", a `v`, or one of the configured names, as in
//! `since Sherpa 0.7.0`. An IP address or a prefix (`10.0.0.0/24`) is not a version.

use std::iter;

use crate::lex::Kind;
use crate::source::Source;

const WORDS: [&str; 7] = ["since", "from", "as of", "before", "after", "until", "in"];

const TWO_PART_WORDS: [&str; 3] = ["since", "before", "as of"];

/// The lines of `source` with a version number after one of the words.
pub fn lines(source: &Source, names: &[String]) -> Vec<usize> {
    let names: Vec<String> = names.iter().map(|name| name.to_lowercase()).collect();
    let mut lines = Vec::new();
    for token in &source.tokens {
        if !matches!(token.kind, Kind::Comment | Kind::Str) {
            continue;
        }
        let lower = source.text[token.range.clone()].to_ascii_lowercase();
        lines.extend(
            version_offsets(&lower, &names)
                .map(|offset| source.line_of(token.range.start + offset)),
        );
    }
    lines
}

/// The offsets in `lower` of each word that a version number follows.
fn version_offsets<'a>(lower: &'a str, names: &'a [String]) -> impl Iterator<Item = usize> + 'a {
    WORDS.iter().flat_map(move |word| {
        lower
            .match_indices(word)
            .map(|(start, _)| start)
            .filter(move |&start| {
                !lower[..start].ends_with(is_word_char)
                    && starts_with_version(&lower[start + word.len()..], word, names)
            })
    })
}

/// Whether `rest`, the text after `word`, opens with a space and a version number.
fn starts_with_version(rest: &str, word: &str, names: &[String]) -> bool {
    let Some(mut rest) = strip_spaces(rest) else {
        return false;
    };
    for prefix in iter::once("version").chain(names.iter().map(String::as_str)) {
        if let Some(after) = rest.strip_prefix(prefix).and_then(strip_spaces) {
            rest = after;
            break;
        }
    }
    let number = rest.strip_prefix('v').unwrap_or(rest);
    let end = number
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(number.len());
    let tail = &number[end..];
    let number = number[..end].trim_end_matches('.');
    let parts: Vec<&str> = number.split('.').collect();
    let min_parts = if TWO_PART_WORDS.contains(&word) { 2 } else { 3 };
    (min_parts..=3).contains(&parts.len())
        && parts.iter().all(|part| !part.is_empty())
        && !tail.starts_with('/')
        && !tail.starts_with(is_word_char)
}

/// `text` without its leading spaces and tabs, or `None` when it does not start with one.
fn strip_spaces(text: &str) -> Option<&str> {
    let rest = text.trim_start_matches([' ', '\t']);
    (rest.len() < text.len()).then_some(rest)
}

/// Whether `c` can be part of a word or an identifier.
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

#[cfg(test)]
mod tests {
    use super::version_offsets;

    /// Whether `text` holds a version number after one of the words.
    fn found(text: &str, names: &[&str]) -> bool {
        let names: Vec<String> = names.iter().map(|name| (*name).to_owned()).collect();
        version_offsets(&text.to_ascii_lowercase(), &names)
            .next()
            .is_some()
    }

    #[test]
    fn three_part_number_after_any_word_is_found() {
        for word in ["since", "from", "as of", "before", "after", "until", "in"] {
            assert!(found(&format!("x {word} 1.2.3 y"), &[]), "{word}");
        }
    }

    #[test]
    fn two_part_number_counts_after_since_before_and_as_of_only() {
        assert!(found("Since 0.7 it caches", &[]));
        assert!(found("before v2.1", &[]));
        assert!(found("as of version 3.0", &[]));
        assert!(!found("wait until 2.5 seconds", &[]));
        assert!(!found("in 1.5 ms", &[]));
    }

    #[test]
    fn configured_name_may_sit_before_the_number() {
        assert!(found("since Sherpa 0.7.0", &["sherpa"]));
        assert!(!found("since Sherpa 0.7.0", &[]));
    }

    #[test]
    fn addresses_and_other_words_are_not_versions() {
        assert!(!found("in 10.0.0.0/24", &[]));
        assert!(!found("in 10.0.0.1", &[]));
        assert!(!found("within 1.2.3", &[]));
        assert!(!found("in 1.2.3rc", &[]));
        assert!(!found("listed in 4 places", &[]));
    }
}
