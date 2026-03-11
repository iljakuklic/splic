use std::collections::HashMap;

use anyhow::{Result, anyhow};

use crate::core::{self, Lvl};
use crate::parser::ast::{self, Phase};

/// Elaboration context.
///
/// `'core` is the lifetime of the core arena that owns all elaborated IR.
/// The source AST lifetime `'src` only appears in method signatures where
/// surface terms are passed in — it does not need to be on the struct itself.
pub struct Ctx<'core> {
    /// Arena for allocating core terms
    arena: &'core bumpalo::Bump,
    /// Local variables: (source name, core type)
    /// Indexed by De Bruijn level (0 = outermost in current scope, len-1 = most recent)
    locals: Vec<(&'core str, &'core core::Term<'core>)>,
    /// Global function signatures: name -> signature
    globals: HashMap<&'core str, core::FunSig<'core>>,
    /// Current phase (Meta or Object)
    phase: Phase,
}

impl<'core> Ctx<'core> {
    pub fn new(
        arena: &'core bumpalo::Bump,
        globals: HashMap<&'core str, core::FunSig<'core>>,
        phase: Phase,
    ) -> Self {
        Ctx {
            arena,
            locals: Vec::new(),
            globals,
            phase,
        }
    }

    /// Allocate a term in the core arena
    fn alloc(&self, term: core::Term<'core>) -> &'core core::Term<'core> {
        self.arena.alloc(term)
    }

    /// Allocate a slice in the core arena
    fn alloc_slice<T>(
        &self,
        items: impl IntoIterator<Item = T, IntoIter: ExactSizeIterator>,
    ) -> &'core [T] {
        self.arena.alloc_slice_fill_iter(items)
    }

    /// Push a local variable onto the context
    fn push_local(&mut self, name: &'core str, ty: &'core core::Term<'core>) {
        self.locals.push((name, ty));
    }

    /// Pop the last local variable
    fn pop_local(&mut self) {
        self.locals.pop();
    }

    /// Look up a variable by name, returning its (level, type).
    /// Searches from the most recently pushed variable inward to handle shadowing.
    /// Level is the index from the start of the vec (outermost = 0, most recent = len-1).
    fn lookup_local(&self, name: &str) -> Option<(Lvl, &'core core::Term<'core>)> {
        for (i, (local_name, ty)) in self.locals.iter().enumerate().rev() {
            if *local_name == name {
                return Some((Lvl(i), ty));
            }
        }
        None
    }

    /// Get the current depth of the locals stack
    fn depth(&self) -> usize {
        self.locals.len()
    }
}

/// Elaborate the entire program in two passes
pub fn elaborate_program<'src, 'core>(
    arena: &'core bumpalo::Bump,
    program: &ast::Program<'src>,
) -> Result<core::Program<'core>> {
    // TODO: Pass 1 - collect signatures
    // TODO: Pass 2 - elaborate bodies
    todo!()
}

/// Infer the type of a surface term, returning (elaborated term, its type)
pub fn infer<'src, 'core>(
    ctx: &mut Ctx<'core>,
    term: &'src ast::Term<'src>,
) -> Result<(&'core core::Term<'core>, &'core core::Term<'core>)> {
    // TODO: implement inference
    Err(anyhow!("infer not yet implemented"))
}

/// Check a surface term against an expected type, returning the elaborated term
pub fn check<'src, 'core>(
    ctx: &mut Ctx<'core>,
    term: &'src ast::Term<'src>,
    expected: &'core core::Term<'core>,
) -> Result<&'core core::Term<'core>> {
    // TODO: implement checking
    Err(anyhow!("check not yet implemented"))
}

#[cfg(test)]
mod test;
