use std::collections::HashMap;

use crate::common::de_bruijn;
use crate::common::env::Env;
use crate::core::{self, value};

/// How a global name is bound: either a meta-level definition (constant or function)
/// whose type is a `Term`, or a code function whose calling convention is stored directly.
#[derive(Debug, Clone)]
pub enum GlobalEntry<'names, 'core> {
    /// Meta-level definition. The stored term is the type (Pi for functions, any type for constants).
    Meta(&'core core::Term<'names, 'core>),
    /// Object-level function. Params and return type are stored directly; no first-class type term.
    CodeFn {
        params: &'core [(&'names core::Name, &'core core::Term<'names, 'core>)],
        ret_ty: &'core core::Term<'names, 'core>,
    },
}

/// A single entry in the elaboration context.
#[derive(Clone, Debug)]
pub struct CtxEntry<'names, 'core> {
    /// Variable name (for lookup and error messages).
    pub name: &'names core::Name,
    /// Type of the variable as a semantic value.
    pub ty: value::Value<'names, 'core>,
    /// Value of the variable in the evaluation environment.
    /// For lambda/Pi parameters this is `Rigid(level)`; for `let` bindings it is
    /// the evaluated expression.
    pub val: value::Value<'names, 'core>,
}

/// Elaboration context.
///
/// `'core` is the lifetime of the core arena that owns all elaborated IR.
/// The source AST lifetime `'src` only appears in method signatures where
/// surface terms are passed in — it does not need to be on the struct itself.
///
/// Phase is not stored here — it is threaded as an argument to `infer`/`check`
/// since it shifts locally when entering `Quote`, `Splice`, or `Lift`.
#[derive(Debug)]
pub struct Ctx<'names, 'core, 'globals> {
    /// Arena for allocating core terms
    pub arena: &'core bumpalo::Bump,

    /// Local variable bindings (oldest first), each carrying name, type, and value.
    pub locals: Env<CtxEntry<'names, 'core>>,

    /// Global definitions: name -> how it was bound.
    /// Borrowed independently of the arena so the map can live on the stack.
    pub globals: &'globals HashMap<&'names core::Name, GlobalEntry<'names, 'core>>,
}

impl<'names, 'core, 'globals> Ctx<'names, 'core, 'globals> {
    pub const fn new(
        arena: &'core bumpalo::Bump,
        globals: &'globals HashMap<&'names core::Name, GlobalEntry<'names, 'core>>,
    ) -> Self {
        Ctx {
            arena,
            locals: Env::new(),
            globals,
        }
    }

    /// Allocate a term in the core arena
    pub fn alloc(&self, term: core::Term<'names, 'core>) -> &'core core::Term<'names, 'core> {
        self.arena.alloc(term)
    }

    /// Allocate a slice in the core arena
    pub fn alloc_slice<T>(
        &self,
        items: impl IntoIterator<Item = T, IntoIter: ExactSizeIterator>,
    ) -> &'core [T] {
        self.arena.alloc_slice_fill_iter(items)
    }

    /// Current De Bruijn depth — always equal to the number of locals.
    pub const fn depth(&self) -> de_bruijn::Depth {
        self.locals.depth()
    }

    /// Push a local variable onto the context, given its type as a term.
    /// Evaluates the type term in the current environment.
    pub fn push_local(&mut self, name: &'names core::Name, ty: &'core core::Term<'names, 'core>) {
        let ty_val = value::eval(self.arena, &self.value_env(), ty);
        self.push_local_val(name, ty_val);
    }

    /// Push a local variable onto the context, given its type as a Value.
    /// The variable itself is a fresh rigid (neutral) variable — use for lambda/pi params.
    pub fn push_local_val(
        &mut self,
        name: &'names core::Name,
        ty_val: value::Value<'names, 'core>,
    ) {
        let lvl = self.depth().as_lvl();
        self.locals.push(CtxEntry {
            name,
            ty: ty_val,
            val: value::Value::Rigid(lvl),
        });
    }

    /// Push a let binding: the variable has a known value in the environment.
    /// Use for `let x = e` bindings so that dependent references to `x` evaluate correctly.
    pub fn push_let_binding(
        &mut self,
        name: &'names core::Name,
        ty_val: value::Value<'names, 'core>,
        expr_val: value::Value<'names, 'core>,
    ) {
        self.locals.push(CtxEntry {
            name,
            ty: ty_val,
            val: expr_val,
        });
    }

    /// Pop the last local variable
    pub fn pop_local(&mut self) {
        self.locals.pop();
    }

    /// Truncate the local context back to a previously saved depth.
    pub fn truncate(&mut self, depth: de_bruijn::Depth) {
        self.locals.truncate(depth);
    }

    /// Push multiple local variables in order, each via `push_local`.
    pub fn extend(
        &mut self,
        iter: impl IntoIterator<Item = (&'names core::Name, &'core core::Term<'names, 'core>)>,
    ) {
        for (name, ty) in iter {
            self.push_local(name, ty);
        }
    }

    /// Look up a variable by name, returning its (index, type as Value).
    /// Searches from the most recently pushed variable inward to handle shadowing.
    pub fn lookup_local(
        &self,
        name: &core::Name,
    ) -> Option<(de_bruijn::Ix, &value::Value<'names, 'core>)> {
        let (_, ix, entry) = self.locals.lookup(name, |e| e.name)?;
        Some((ix, &entry.ty))
    }

    /// Helper to create a lifted type \[\[T\]\]
    pub fn lift_ty(
        &self,
        inner: &'core core::Term<'names, 'core>,
    ) -> &'core core::Term<'names, 'core> {
        self.arena.alloc(core::Term::Lift(inner))
    }

    /// Collect the values of all local bindings as an `Env<Value>` for use with `value::eval`.
    pub fn value_env(&self) -> value::Env<'names, 'core> {
        let mut env = value::Env::with_capacity(self.locals.depth().as_usize());
        for entry in self.locals.iter_by_lvl() {
            env.push(entry.val.clone());
        }
        env
    }

    /// Evaluate a term in the current environment.
    pub fn eval(&self, term: &'core core::Term<'names, 'core>) -> value::Value<'names, 'core> {
        value::eval(self.arena, &self.value_env(), term)
    }

    /// Quote a value back to a term at the current depth.
    pub fn quote_val(&self, val: &value::Value<'names, 'core>) -> &'core core::Term<'names, 'core> {
        value::quote(self.arena, self.depth(), val)
    }
}
