//! Module docs.

/// Type docs.
#[derive(Debug, Clone)]
pub struct Named {
    /// Field doc.
    pub a: u8,
    // Plain comment.
    b: u16,
    c: u32, // Trailing comment.
    /** Block doc. */
    d: u64,
    /// Doc above a multi-line attribute.
    #[serde(
        default,
        rename = "e"
    )]
    e: String,
    #[doc = "Doc attribute."]
    f: bool,
    #[serde(rename = "a//b")]
    g: &'static str,
}

/// Tuple type docs.
pub struct Tuple<T: Fn(u8)>(
    /// Tuple field doc.
    pub T,
    String, // Trailing comment.
);

/// Enum docs.
pub enum Kind {
    /// Variant doc.
    Unit,
    Tuple(u8), // Trailing comment.
    /* Block comment. */
    Struct {
        /// Variant field doc.
        x: u8,
    },
}

/// Command line docs.
#[derive(
    Debug,
    clap::Parser,
)]
pub struct Cli {
    /// Help text.
    verbose: bool,
}

#[derive(Subcommand)]
enum Command {
    /// Help text.
    Run {
        /// Help text.
        fast: bool,
    },
}

/// Impl docs.
impl Named {
    /// Function docs.
    fn new() -> Self {
        // Body comment.
        let brace = '{';
        /// Local type docs.
        struct Local {
            /// Local field doc.
            x: u8,
        }
        todo!() // Body comment.
    }
}
