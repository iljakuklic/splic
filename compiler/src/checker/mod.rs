use std::collections::HashMap;

use anyhow::{anyhow, Result};

use crate::core::{self, IntType, IntWidth, Lvl, Prim};
use crate::parser::ast::{self, Phase};

/// Elaboration context.
///
/// `'core` is the lifetime of the core arena that owns all elaborated IR.
/// The source AST lifetime `'src` only appears in method signatures where
/// surface terms are passed in — it does not need to be on the struct itself.
///
/// Phase is not stored here — it is threaded as an argument to `infer`/`check`
/// since it shifts locally when entering `Quote`, `Splice`, or `Lift`.
pub struct Ctx<'core> {
    /// Arena for allocating core terms
    arena: &'core bumpalo::Bump,
    /// Local variables: (source name, core type)
    /// Indexed by De Bruijn level (0 = outermost in current scope, len-1 = most recent)
    locals: Vec<(&'core str, &'core core::Term<'core>)>,
    /// Global function signatures: name -> signature
    globals: HashMap<&'core str, core::FunSig<'core>>,
}

impl<'core> Ctx<'core> {
    pub fn new(
        arena: &'core bumpalo::Bump,
        globals: HashMap<&'core str, core::FunSig<'core>>,
    ) -> Self {
        Ctx {
            arena,
            locals: Vec::new(),
            globals,
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

    /// Helper to create a u64 type term (meta phase)
    pub fn u64_ty(&self) -> &'core core::Term<'core> {
        self.arena.alloc(core::Term::Prim(Prim::IntTy(IntType::new(
            IntWidth::U64,
            Phase::Meta,
        ))))
    }

    /// Helper to create a u32 type term (meta phase)
    pub fn u32_ty(&self) -> &'core core::Term<'core> {
        self.arena.alloc(core::Term::Prim(Prim::IntTy(IntType::new(
            IntWidth::U32,
            Phase::Meta,
        ))))
    }

    /// Helper to create a u1 type term (meta phase)
    pub fn u1_ty(&self) -> &'core core::Term<'core> {
        self.arena.alloc(core::Term::Prim(Prim::IntTy(IntType::new(
            IntWidth::U1,
            Phase::Meta,
        ))))
    }

    /// Helper to create a Type (meta universe) term
    pub fn type_ty(&self) -> &'core core::Term<'core> {
        self.arena.alloc(core::Term::Prim(Prim::U(Phase::Meta)))
    }

    /// Helper to create a VmType (object universe) term
    pub fn vm_type_ty(&self) -> &'core core::Term<'core> {
        self.arena.alloc(core::Term::Prim(Prim::U(Phase::Object)))
    }

    /// Helper to create a lifted type [[T]]
    pub fn lift_ty(&self, inner: &'core core::Term<'core>) -> &'core core::Term<'core> {
        self.arena.alloc(core::Term::Lift(inner))
    }
}

/// Elaborate a surface type expression into a core `Term`.
///
/// Only the forms that can appear in top-level type positions are handled here:
/// primitive type names (`u1`, `u32`, `u64`, `Type`, `VmType`) and `[[T]]`.
/// This is intentionally restricted — full term elaboration happens in `infer`/`check`.
fn elaborate_ty<'src, 'core>(
    arena: &'core bumpalo::Bump,
    phase: Phase,
    ty: &'src ast::Term<'src>,
) -> Result<&'core core::Term<'core>> {
    match ty {
        ast::Term::Var(name) => {
            let prim = match name.as_str() {
                "u1" => Prim::IntTy(IntType::new(IntWidth::U1, phase)),
                "u8" => Prim::IntTy(IntType::new(IntWidth::U8, phase)),
                "u16" => Prim::IntTy(IntType::new(IntWidth::U16, phase)),
                "u32" => Prim::IntTy(IntType::new(IntWidth::U32, phase)),
                "u64" => Prim::IntTy(IntType::new(IntWidth::U64, phase)),
                "Type" => Prim::U(Phase::Meta),
                "VmType" => Prim::U(Phase::Object),
                other => return Err(anyhow!("unknown type `{other}`")),
            };
            Ok(arena.alloc(core::Term::Prim(prim)))
        }
        ast::Term::Lift(inner) => {
            // `[[T]]` — inner type must be an object type
            let inner_ty = elaborate_ty(arena, Phase::Object, inner)?;
            Ok(arena.alloc(core::Term::Lift(inner_ty)))
        }
        _ => Err(anyhow!("expected a type expression")),
    }
}

/// Pass 1: collect all top-level function signatures into a globals table.
///
/// Type annotations on parameters and return types are elaborated here so that
/// pass 2 (body elaboration) has fully-typed signatures available for all
/// functions, including forward references.
pub(crate) fn collect_signatures<'src, 'core>(
    arena: &'core bumpalo::Bump,
    program: &ast::Program<'src>,
) -> Result<HashMap<&'core str, core::FunSig<'core>>> {
    let mut globals: HashMap<&'core str, core::FunSig<'core>> = HashMap::new();

    for func in program.functions {
        let name: &'core str = arena.alloc_str(func.name.as_str());

        if globals.contains_key(name) {
            return Err(anyhow!("duplicate function name `{name}`"));
        }

        // Elaborate parameter types in the function's own phase
        let params: &'core [(&'core str, &'core core::Term<'core>)] = arena
            .alloc_slice_try_fill_iter(func.params.iter().map(|p| -> Result<_> {
                let param_name: &'core str = arena.alloc_str(p.name.as_str());
                let param_ty = elaborate_ty(arena, func.phase, p.ty)?;
                Ok((param_name, param_ty))
            }))?;

        let ret_ty = elaborate_ty(arena, func.phase, func.ret_ty)?;

        globals.insert(
            name,
            core::FunSig {
                params,
                ret_ty,
                phase: func.phase,
            },
        );
    }

    Ok(globals)
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

pub fn infer<'src, 'core>(
    ctx: &mut Ctx<'core>,
    phase: Phase,
    term: &'src ast::Term<'src>,
) -> Result<(&'core core::Term<'core>, &'core core::Term<'core>)> {
    Err(anyhow!("infer not yet implemented"))
}

pub fn check<'src, 'core>(
    ctx: &mut Ctx<'core>,
    phase: Phase,
    term: &'src ast::Term<'src>,
    expected: &'core core::Term<'core>,
) -> Result<&'core core::Term<'core>> {
    Err(anyhow!("check not yet implemented"))
}

#[cfg(test)]
mod test;
