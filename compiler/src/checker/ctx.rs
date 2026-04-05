use std::collections::HashMap;

use crate::common::de_bruijn;
use crate::core::{self, value};

/// Elaboration context.
///
/// `'core` is the lifetime of the core arena that owns all elaborated IR.
/// The source AST lifetime `'src` only appears in method signatures where
/// surface terms are passed in — it does not need to be on the struct itself.
///
/// Phase is not stored here — it is threaded as an argument to `infer`/`check`
/// since it shifts locally when entering `Quote`, `Splice`, or `Lift`.
#[derive(Debug)]
pub struct Ctx<'core, 'globals> {
    /// Arena for allocating core terms
    pub arena: &'core bumpalo::Bump,

    /// Local variable names (oldest first), for error messages.
    pub names: Vec<&'core core::Name>,

    /// Evaluation environment (oldest first): values of locals.
    /// `env[env.len() - 1 - ix]` = value of `Var(Ix(ix))`.
    pub env: value::Env<'core>,

    /// Types of locals as semantic values (oldest first).
    /// `types[types.len() - 1 - ix]` = type of `Var(Ix(ix))`.
    pub types: Vec<value::Value<'core>>,

    /// Global function types: name -> Pi term.
    /// Storing `&Term` (always a Pi) unifies type lookup for globals and locals.
    /// Borrowed independently of the arena so the map can live on the stack.
    pub globals: &'globals HashMap<&'core core::Name, &'core core::Pi<'core>>,
}

impl<'core, 'globals> Ctx<'core, 'globals> {
    pub const fn new(
        arena: &'core bumpalo::Bump,
        globals: &'globals HashMap<&'core core::Name, &'core core::Pi<'core>>,
    ) -> Self {
        Ctx {
            arena,
            names: Vec::new(),
            env: Vec::new(),
            types: Vec::new(),
            globals,
        }
    }

    /// Allocate a term in the core arena
    pub fn alloc(&self, term: core::Term<'core>) -> &'core core::Term<'core> {
        self.arena.alloc(term)
    }

    /// Allocate a slice in the core arena
    pub fn alloc_slice<T>(
        &self,
        items: impl IntoIterator<Item = T, IntoIter: ExactSizeIterator>,
    ) -> &'core [T] {
        self.arena.alloc_slice_fill_iter(items)
    }

    /// Current De Bruijn depth — always equal to `env.len()`.
    pub const fn depth(&self) -> de_bruijn::Depth {
        de_bruijn::Depth::new(self.env.len())
    }

    /// Push a local variable onto the context, given its type as a term.
    /// Evaluates the type term in the current environment.
    pub fn push_local(&mut self, name: &'core core::Name, ty: &'core core::Term<'core>) {
        let ty_val = value::eval(self.arena, &self.env, ty);
        self.push_local_val(name, ty_val);
    }

    /// Push a local variable onto the context, given its type as a Value.
    /// The variable itself is a fresh rigid (neutral) variable — use for lambda/pi params.
    pub fn push_local_val(&mut self, name: &'core core::Name, ty_val: value::Value<'core>) {
        self.env.push(value::Value::Rigid(self.depth().as_lvl()));
        self.types.push(ty_val);
        self.names.push(name);
    }

    /// Push a let binding: the variable has a known value in the environment.
    /// Use for `let x = e` bindings so that dependent references to `x` evaluate correctly.
    pub fn push_let_binding(
        &mut self,
        name: &'core core::Name,
        ty_val: value::Value<'core>,
        expr_val: value::Value<'core>,
    ) {
        self.env.push(expr_val);
        self.types.push(ty_val);
        self.names.push(name);
    }

    /// Pop the last local variable
    pub fn pop_local(&mut self) {
        self.names.pop();
        self.env.pop();
        self.types.pop();
    }

    /// Look up a variable by name, returning its (index, type as Value).
    /// Searches from the most recently pushed variable inward to handle shadowing.
    pub fn lookup_local(
        &self,
        name: &'_ core::Name,
    ) -> Option<(de_bruijn::Ix, &value::Value<'core>)> {
        for (i, local_name) in self.names.iter().enumerate().rev() {
            if *local_name == name {
                let ix = de_bruijn::Lvl::new(i).ix_at(self.depth());
                let ty = self
                    .types
                    .get(i)
                    .expect("types and names are always the same length");
                return Some((ix, ty));
            }
        }
        None
    }

    /// Helper to create a lifted type [[T]]
    pub fn lift_ty(&self, inner: &'core core::Term<'core>) -> &'core core::Term<'core> {
        self.arena.alloc(core::Term::Lift(inner))
    }

    /// Evaluate a term in the current environment.
    pub fn eval(&self, term: &'core core::Term<'core>) -> value::Value<'core> {
        value::eval(self.arena, &self.env, term)
    }

    /// Quote a value back to a term at the current depth.
    pub fn quote_val(&self, val: &value::Value<'core>) -> &'core core::Term<'core> {
        value::quote(self.arena, self.depth(), val)
    }
}
