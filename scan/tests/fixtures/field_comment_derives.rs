//! Exempt derives are found however the derive is written.

#[cfg_attr(feature = "cli", derive(Debug, clap::Args))]
pub struct BehindCfgAttr {
    /// Help text.
    verbose: bool,
}

#[derive(Debug)]
#[derive(Subcommand)]
pub enum SecondDerive {
    /// Help text.
    Run,
}

#[derive(Debug, ParserLike)]
pub struct NotExempt {
    /// Field doc.
    a: u8,
}

pub union Bits {
    /// Union fields are not checked.
    int: u32,
    float: f32,
}

macro_rules! typed {
    () => {
        pub struct InMacro {
            /// A macro's input is not parsed as items.
            a: u8,
        }
    };
}

#[cfg(windows)]
pub struct LeftOutByCfg {
    a: u8, // Checked all the same.
}
