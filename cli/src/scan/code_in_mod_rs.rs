//! `code_in_mod_rs`: a file named `mod.rs` holds only docs, attributes, `mod` declarations
//! (`mod name;`) and re-exports (`use` with a visibility).

use proc_macro2::Span;
use syn::spanned::Spanned;
use syn::{Item, Visibility};

use crate::source::Source;

const MOD_FILE_NAME: &str = "mod.rs";

/// The lines of `source`, when it is a `mod.rs`, that start an item it may not hold.
pub fn lines(source: &Source, file: &syn::File) -> Vec<usize> {
    if source.path.rsplit('/').next() != Some(MOD_FILE_NAME) {
        return Vec::new();
    }
    file.items
        .iter()
        .filter(|item| !allowed(item))
        .map(|item| source.line_of(keyword_span(item).byte_range().start))
        .collect()
}

/// Whether a `mod.rs` may hold `item`.
fn allowed(item: &Item) -> bool {
    match item {
        Item::Mod(item) => item.content.is_none(),
        Item::Use(item) => !matches!(item.vis, Visibility::Inherited),
        _ => false,
    }
}

/// The span of the keyword that opens `item`, past its attributes.
fn keyword_span(item: &Item) -> Span {
    match item {
        Item::Const(item) => item.const_token.span,
        Item::Enum(item) => item.enum_token.span,
        Item::ExternCrate(item) => item.extern_token.span,
        Item::Fn(item) => item.sig.fn_token.span,
        Item::ForeignMod(item) => item.abi.extern_token.span,
        Item::Impl(item) => item.impl_token.span,
        Item::Macro(item) => item.mac.path.span(),
        Item::Mod(item) => item.mod_token.span,
        Item::Static(item) => item.static_token.span,
        Item::Struct(item) => item.struct_token.span,
        Item::Trait(item) => item.trait_token.span,
        Item::TraitAlias(item) => item.trait_token.span,
        Item::Type(item) => item.type_token.span,
        Item::Union(item) => item.union_token.span,
        Item::Use(item) => item.use_token.span,
        _ => item.span(),
    }
}
