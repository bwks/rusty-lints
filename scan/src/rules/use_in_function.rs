//! `use_in_function`: a `use` declaration is never written inside a block, such as a function
//! body, a closure or a nested block.

use syn::visit::{self, Visit};
use syn::{Block, Item, Stmt};

use crate::source::Source;

struct Uses {
    starts: Vec<usize>,
}

impl<'ast> Visit<'ast> for Uses {
    fn visit_block(&mut self, block: &'ast Block) {
        for stmt in &block.stmts {
            if let Stmt::Item(Item::Use(item)) = stmt {
                self.starts.push(item.use_token.span.byte_range().start);
            }
        }
        visit::visit_block(self, block);
    }
}

/// The lines of `source` that start a `use` declaration inside a block.
pub fn lines(source: &Source, file: &syn::File) -> Vec<usize> {
    let mut uses = Uses { starts: Vec::new() };
    uses.visit_file(file);
    uses.starts
        .into_iter()
        .map(|start| source.line_of(start))
        .collect()
}
