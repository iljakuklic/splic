use std::collections::HashMap;

use crate::core::{self, IntType, IntWidth, Ix, Lvl, Phase, Pi, Prim, value};

/// Elaboration context.
///
/// `'core` is the lifetime of the core arena that owns all elaborated IR.
/// The source AST lifetime `'src` only appears in method signatures where
/// surface terms are passed in — it does not need to be on the struct itself.
///
/// Phase is not stored here — it is threaded as an argument to `infer`/`check`
/// since it shifts locally when entering `Quote`, `Splice`, or `Lift`.
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
    /// Current De Bruijn level (= `env.len()` = `types.len()`).
    pub lvl: Lvl,
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
            lvl: Lvl::new(0),
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

    /// Push a local variable onto the context, given its type as a term.
    /// Evaluates the type term in the current environment.
    pub fn push_local(&mut self, name: &'core core::Name, ty: &'core core::Term<'core>) {
        let ty_val = value::eval(self.arena, &self.env, ty);
        self.env.push(value::Value::Rigid(self.lvl));
        self.types.push(ty_val);
        self.lvl = self.lvl.succ();
        self.names.push(name);
    }

    /// Push a local variable onto the context, given its type as a Value.
    /// The variable itself is a fresh rigid (neutral) variable — use for lambda/pi params.
    pub fn push_local_val(&mut self, name: &'core core::Name, ty_val: value::Value<'core>) {
        self.env.push(value::Value::Rigid(self.lvl));
        self.types.push(ty_val);
        self.lvl = self.lvl.succ();
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
        self.lvl = self.lvl.succ();
        self.names.push(name);
    }

    /// Pop the last local variable
    pub fn pop_local(&mut self) {
        self.names.pop();
        self.env.pop();
        self.types.pop();
        self.lvl = Lvl(self.lvl.0 - 1);
    }

    /// Look up a variable by name, returning its (index, type as Value).
    /// Searches from the most recently pushed variable inward to handle shadowing.
    pub fn lookup_local(&self, name: &'_ core::Name) -> Option<(Ix, &value::Value<'core>)> {
        for (i, local_name) in self.names.iter().enumerate().rev() {
            if *local_name == name {
                let ix = Lvl(i).ix_at_depth(self.lvl);
                let ty = self
                    .types
                    .get(i)
                    .expect("types and names are always the same length");
                return Some((ix, ty));
            }
        }
        None
    }

    /// Get the current depth of the locals stack
    pub const fn depth(&self) -> usize {
        self.lvl.0
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
        value::quote(self.arena, self.lvl, val)
    }

    /// Recover the type of an already-elaborated core term as a semantic Value.
    ///
    /// Precondition: `term` was produced by `infer` or `check` in a context
    /// compatible with `self`.  Panics on typechecker invariant violations.
    pub fn val_type_of(&self, term: &'core core::Term<'core>) -> value::Value<'core> {
        match term {
            // Literal or arithmetic/bitwise op: type is (or returns) the integer type.
            core::Term::Lit(_, it)
            | core::Term::Prim(
                Prim::Add(it)
                | Prim::Sub(it)
                | Prim::Mul(it)
                | Prim::Div(it)
                | Prim::BitAnd(it)
                | Prim::BitOr(it)
                | Prim::BitNot(it),
            ) => value::Value::Prim(Prim::IntTy(*it)),

            // Variable: look up type by De Bruijn index.
            core::Term::Var(ix) => {
                let i = self
                    .types
                    .len()
                    .checked_sub(1 + ix.0)
                    .expect("Var index out of range (typechecker invariant)");
                self.types
                    .get(i)
                    .expect("Var index out of range (typechecker invariant)")
                    .clone()
            }

            // Primitive types inhabit the relevant universe.
            core::Term::Prim(Prim::IntTy(it)) => value::Value::Prim(Prim::U(it.phase)),
            // Type, VmType, and [[T]] all inhabit Type (meta universe).
            core::Term::Prim(Prim::U(_)) | core::Term::Lift(_) | core::Term::Pi(_) => {
                value::Value::Prim(Prim::U(Phase::Meta))
            }

            // Comparison ops return u1 at the operand phase.
            core::Term::Prim(
                Prim::Eq(it)
                | Prim::Ne(it)
                | Prim::Lt(it)
                | Prim::Gt(it)
                | Prim::Le(it)
                | Prim::Ge(it),
            ) => value::Value::Prim(Prim::IntTy(IntType {
                width: IntWidth::U1,
                phase: it.phase,
            })),

            // Embed: IntTy(w, Meta) -> [[IntTy(w, Object)]]
            core::Term::Prim(Prim::Embed(w)) => {
                let obj_int_ty = value::Value::Prim(Prim::IntTy(IntType {
                    width: *w,
                    phase: Phase::Object,
                }));
                value::Value::Lift(self.arena.alloc(obj_int_ty))
            }

            // Global reference: look up its Pi type and evaluate.
            core::Term::Global(name) => {
                let pi = self
                    .globals
                    .get(name)
                    .copied()
                    .expect("Global with unknown name (typechecker invariant)");
                value::eval_pi(self.arena, &self.env, pi)
            }

            // App: compute return type via NbE.
            core::Term::App(app) => match app.func {
                core::Term::Prim(prim) => match prim {
                    Prim::Add(it)
                    | Prim::Sub(it)
                    | Prim::Mul(it)
                    | Prim::Div(it)
                    | Prim::BitAnd(it)
                    | Prim::BitOr(it)
                    | Prim::BitNot(it) => value::Value::Prim(Prim::IntTy(*it)),
                    Prim::Eq(it)
                    | Prim::Ne(it)
                    | Prim::Lt(it)
                    | Prim::Gt(it)
                    | Prim::Le(it)
                    | Prim::Ge(it) => value::Value::Prim(Prim::IntTy(IntType {
                        width: IntWidth::U1,
                        phase: it.phase,
                    })),
                    Prim::Embed(w) => {
                        let obj_int_ty = value::Value::Prim(Prim::IntTy(IntType {
                            width: *w,
                            phase: Phase::Object,
                        }));
                        value::Value::Lift(self.arena.alloc(obj_int_ty))
                    }
                    Prim::IntTy(_) | Prim::U(_) => {
                        unreachable!("type-level prim in App (typechecker invariant)")
                    }
                },
                _ => {
                    // Compute return type by peeling Pi closures via NbE.
                    let mut pi_val = self.val_type_of(app.func);
                    for arg in app.args {
                        match pi_val {
                            value::Value::Pi(vpi) => {
                                let arg_val = self.eval(arg);
                                pi_val = value::inst(self.arena, &vpi.closure, arg_val);
                            }
                            _ => unreachable!("App func must have Pi type (typechecker invariant)"),
                        }
                    }
                    pi_val
                }
            },

            // Lam: synthesise Pi from params and body type.
            core::Term::Lam(lam) => {
                // Compute the Pi type for this Lam.
                // Build a Pi value matching the Lam's structure.
                // Since we need the full Pi type, compute it directly from lam params.
                let mut env2 = self.env.clone();
                let mut types2 = self.types.clone();
                let mut lvl2 = self.lvl;
                let mut names2 = self.names.clone();
                let mut elaborated_param_types: Vec<value::Value<'core>> = Vec::new();
                for &(pname, pty) in lam.params {
                    let ty_val = value::eval(self.arena, &env2, pty);
                    elaborated_param_types.push(ty_val.clone());
                    env2.push(value::Value::Rigid(lvl2));
                    types2.push(ty_val);
                    lvl2 = lvl2.succ();
                    names2.push(pname);
                }
                let body_ty_term = {
                    // Compute type of body in extended env
                    let fake_ctx = Ctx {
                        arena: self.arena,
                        names: names2,
                        env: env2,
                        types: types2,
                        lvl: lvl2,
                        globals: self.globals,
                    };
                    fake_ctx.val_type_of(lam.body)
                };
                // Quote body type and build Pi term, then eval back to value
                let body_ty_quoted = value::quote(self.arena, lvl2, &body_ty_term);
                // Build a Pi term with the same params
                let params_slice = self.alloc_slice(lam.params.iter().copied());
                let pi_term = self.alloc(core::Term::Pi(Pi {
                    params: params_slice,
                    body_ty: body_ty_quoted,
                    phase: Phase::Meta,
                }));
                self.eval(pi_term)
            }

            // #(t) : [[type_of(t)]]
            core::Term::Quote(inner) => {
                let inner_ty = self.val_type_of(inner);
                value::Value::Lift(self.arena.alloc(inner_ty))
            }

            // $(t) where t : [[T]] — strips the Lift.
            core::Term::Splice(inner) => {
                let inner_ty = self.val_type_of(inner);
                match inner_ty {
                    value::Value::Lift(object_ty) => (*object_ty).clone(),
                    _ => {
                        unreachable!("Splice inner must have Lift type (typechecker invariant)")
                    }
                }
            }

            // let x : T = e in body — type is type_of(body) with x in scope.
            core::Term::Let(core::Let {
                name,
                ty,
                expr,
                body,
            }) => {
                let ty_val = self.eval(ty);
                let expr_val = self.eval(expr);
                let mut env2 = self.env.clone();
                env2.push(expr_val);
                let mut types2 = self.types.clone();
                types2.push(ty_val);
                let mut names2 = self.names.clone();
                names2.push(*name);
                let lvl2 = self.lvl.succ();
                let fake_ctx = Ctx {
                    arena: self.arena,
                    names: names2,
                    env: env2,
                    types: types2,
                    lvl: lvl2,
                    globals: self.globals,
                };
                fake_ctx.val_type_of(body)
            }

            // match: all arms share the same type; recover from the first.
            core::Term::Match(core::Match { scrutinee, arms }) => {
                let arm = arms
                    .first()
                    .expect("Match with no arms (typechecker invariant)");
                match arm.pat {
                    core::Pat::Lit(_) | core::Pat::Wildcard => self.val_type_of(arm.body),
                    core::Pat::Bind(name) => {
                        let scrut_ty = self.val_type_of(scrutinee);
                        // Extend context with scrutinee binding
                        let scrut_ty_term = self.quote_val(&scrut_ty);
                        let mut fake_ctx = Ctx {
                            arena: self.arena,
                            names: self.names.clone(),
                            env: self.env.clone(),
                            types: self.types.clone(),
                            lvl: self.lvl,
                            globals: self.globals,
                        };
                        fake_ctx.push_local(name, scrut_ty_term);
                        fake_ctx.val_type_of(arm.body)
                    }
                }
            }
        }
    }
}
