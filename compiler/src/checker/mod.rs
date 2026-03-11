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
pub struct Ctx<'core, 'globals> {
    /// Arena for allocating core terms
    arena: &'core bumpalo::Bump,
    /// Local variables: (source name, core type)
    /// Indexed by De Bruijn level (0 = outermost in current scope, len-1 = most recent)
    locals: Vec<(&'core str, &'core core::Term<'core>)>,
    /// Global function signatures: name -> signature.
    /// Borrowed independently of the arena so the map can live on the stack.
    globals: &'globals HashMap<&'core str, core::FunSig<'core>>,
}

impl<'core, 'globals> Ctx<'core, 'globals> {
    pub fn new(
        arena: &'core bumpalo::Bump,
        globals: &'globals HashMap<&'core str, core::FunSig<'core>>,
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

/// Pass 2: elaborate all function bodies with the full globals table available.
fn elaborate_bodies<'src, 'core>(
    arena: &'core bumpalo::Bump,
    program: &ast::Program<'src>,
    globals: HashMap<&'core str, core::FunSig<'core>>,
) -> Result<core::Program<'core>> {
    let functions: &'core [core::Function<'core>] =
        arena.alloc_slice_try_fill_iter(program.functions.iter().map(|func| -> Result<_> {
            let name: &'core str = arena.alloc_str(func.name.as_str());
            let sig = globals.get(name).expect("signature missing from pass 1");

            // Build a fresh context borrowing the stack-owned globals map.
            let mut ctx = Ctx::new(arena, &globals);

            // Push parameters as locals so the body can reference them.
            for (pname, pty) in sig.params {
                ctx.push_local(pname, pty);
            }

            // Elaborate the body, checking it against the declared return type.
            let core_body = check(&mut ctx, sig.phase, func.body, sig.ret_ty)?;

            // Re-borrow sig from globals (ctx was consumed in the check above).
            // We need the sig fields for the Function; collect them before moving ctx.
            let core_sig = core::FunSig {
                params: sig.params,
                ret_ty: sig.ret_ty,
                phase: sig.phase,
            };

            Ok(core::Function {
                name,
                sig: core_sig,
                body: core_body,
            })
        }))?;

    Ok(core::Program { functions })
}

/// Elaborate the entire program in two passes
pub fn elaborate_program<'src, 'core>(
    arena: &'core bumpalo::Bump,
    program: &ast::Program<'src>,
) -> Result<core::Program<'core>> {
    let globals = collect_signatures(arena, program)?;
    elaborate_bodies(arena, program, globals)
}

/// Structural equality of core types (no normalisation needed for this prototype).
///
/// Uses pointer equality as a fast path — terms allocated from the same arena
/// slot are guaranteed identical without recursion.
fn types_equal(a: &core::Term<'_>, b: &core::Term<'_>) -> bool {
    if std::ptr::eq(a, b) {
        return true;
    }
    a == b
}

/// Synthesise the type of `term`, returning `(elaborated_term, type)`.
pub fn infer<'src, 'core>(
    ctx: &mut Ctx<'core, '_>,
    phase: Phase,
    term: &'src ast::Term<'src>,
) -> Result<(&'core core::Term<'core>, &'core core::Term<'core>)> {
    match term {
        // ------------------------------------------------------------------ Var
        // Look up the name in locals; return its level and type.
        ast::Term::Var(name) => {
            let name_str = name.as_str();
            // First check if it's a built-in type name — those are inferable too.
            let builtin = match name_str {
                "u1" => Some(Prim::IntTy(IntType::new(IntWidth::U1, phase))),
                "u8" => Some(Prim::IntTy(IntType::new(IntWidth::U8, phase))),
                "u16" => Some(Prim::IntTy(IntType::new(IntWidth::U16, phase))),
                "u32" => Some(Prim::IntTy(IntType::new(IntWidth::U32, phase))),
                "u64" => Some(Prim::IntTy(IntType::new(IntWidth::U64, phase))),
                "Type" => Some(Prim::U(Phase::Meta)),
                "VmType" => Some(Prim::U(Phase::Object)),
                _ => None,
            };
            if let Some(prim) = builtin {
                let term = ctx.alloc(core::Term::Prim(prim));
                // The type of a type is the relevant universe.
                let ty = match prim {
                    Prim::IntTy(_) => ctx.alloc(core::Term::Prim(Prim::U(phase))),
                    Prim::U(Phase::Meta) => ctx.alloc(core::Term::Prim(Prim::U(Phase::Meta))),
                    Prim::U(Phase::Object) => ctx.alloc(core::Term::Prim(Prim::U(Phase::Meta))),
                    _ => unreachable!(),
                };
                return Ok((term, ty));
            }
            // Otherwise look in locals.
            let (lvl, ty) = ctx
                .lookup_local(name_str)
                .ok_or_else(|| anyhow!("unbound variable `{name_str}`"))?;
            let core_term = ctx.alloc(core::Term::Var(lvl));
            Ok((core_term, ty))
        }

        // ------------------------------------------------------------------ Lit
        // Literals have no intrinsic type — they are check-only.
        ast::Term::Lit(_) => Err(anyhow!(
            "cannot infer type of a literal; add a type annotation"
        )),

        // ------------------------------------------------------------------ App { Global }
        // Look up the callee in globals, check each argument, return the return type.
        ast::Term::App {
            func: ast::FunName::Name(name),
            args,
        } => {
            let name_str = name.as_str();
            let sig = ctx
                .globals
                .get(name_str)
                .ok_or_else(|| anyhow!("unknown function `{name_str}`"))?;

            let call_phase = sig.phase;
            let ret_ty = sig.ret_ty;
            let params = sig.params;

            if args.len() != params.len() {
                return Err(anyhow!(
                    "function `{name_str}` expects {} argument(s), got {}",
                    params.len(),
                    args.len()
                ));
            }

            // Check each argument against its declared parameter type.
            let core_args: &'core [&'core core::Term<'core>] = ctx
                .arena
                .alloc_slice_try_fill_iter(args.iter().zip(params.iter()).map(
                    |(arg, (_pname, pty))| -> Result<_> {
                        let core_arg = check(ctx, call_phase, arg, pty)?;
                        Ok(core_arg)
                    },
                ))?;

            let core_term = ctx.alloc(core::Term::App {
                head: core::Head::Global(ctx.arena.alloc_str(name_str)),
                args: core_args,
            });
            Ok((core_term, ret_ty))
        }

        // ------------------------------------------------------------------ App { Prim (BinOp/UnOp) }
        // Primitive ops are check-only — their width is determined by the expected type.
        ast::Term::App {
            func: ast::FunName::BinOp(_) | ast::FunName::UnOp(_),
            ..
        } => Err(anyhow!(
            "cannot infer type of a primitive operation; add a type annotation"
        )),

        // ------------------------------------------------------------------ Lift
        // `[[T]]` — elaborate T at the object phase, type is Type (meta universe).
        ast::Term::Lift(inner) => {
            // The inner expression must be an object type.
            let (core_inner, inner_ty) = infer(ctx, Phase::Object, inner)?;
            // Verify the inner term is indeed a type (inhabits VmType).
            if !types_equal(
                inner_ty,
                ctx.arena.alloc(core::Term::Prim(Prim::U(Phase::Object))),
            ) {
                return Err(anyhow!("argument of `[[...]]` must be an object type"));
            }
            let lift_term = ctx.alloc(core::Term::Lift(core_inner));
            let ty = ctx.alloc(core::Term::Prim(Prim::U(Phase::Meta)));
            Ok((lift_term, ty))
        }

        // ------------------------------------------------------------------ Quote
        // `#(t)` — infer iff the inner term is inferable (phase shifts meta→object).
        ast::Term::Quote(inner) => {
            let (core_inner, inner_ty) = infer(ctx, Phase::Object, inner)?;
            let lift_ty = ctx.alloc(core::Term::Lift(inner_ty));
            let core_term = ctx.alloc(core::Term::Quote(core_inner));
            Ok((core_term, lift_ty))
        }

        // ------------------------------------------------------------------ Splice
        // `$(t)` — infer iff `t` infers as `[[T]]`; result type is `T` (phase shifts object→meta).
        ast::Term::Splice(inner) => {
            let (core_inner, inner_ty) = infer(ctx, Phase::Meta, inner)?;
            match inner_ty {
                core::Term::Lift(object_ty) => {
                    let core_term = ctx.alloc(core::Term::Splice(core_inner));
                    Ok((core_term, object_ty))
                }
                _ => Err(anyhow!(
                    "argument of `$(...)` must have a lifted type `[[T]]`"
                )),
            }
        }

        // ------------------------------------------------------------------ Block (Let*)
        // Elaborate each `let` binding in sequence, then the trailing expression.
        ast::Term::Block { stmts, expr } => {
            let depth_before = ctx.depth();
            let result = infer_block(ctx, phase, stmts, expr);
            // Restore the context depth (pop any locals we pushed).
            while ctx.depth() > depth_before {
                ctx.pop_local();
            }
            result
        }

        // ------------------------------------------------------------------ Match
        // Infer iff all arms are inferable and agree on a common type.
        ast::Term::Match { scrutinee, arms } => {
            let (core_scrutinee, _scrut_ty) = infer(ctx, phase, scrutinee)?;

            if arms.is_empty() {
                return Err(anyhow!("match expression has no arms"));
            }

            let mut common_ty: Option<&'core core::Term<'core>> = None;
            let core_arms: &'core [core::Arm<'core>] =
                ctx.arena
                    .alloc_slice_try_fill_iter(arms.iter().map(|arm| -> Result<_> {
                        let (core_pat, bound_name) = elaborate_pat(ctx, &arm.pat)?;
                        // If the pattern binds a name, push it into locals for the arm body.
                        // We use a placeholder type (scrutinee type) — sufficient for the prototype.
                        let pushed = if let Some(bname) = bound_name {
                            // For now push with the scrutinee type as a placeholder; full
                            // dependent pattern matching is out of prototype scope.
                            ctx.push_local(
                                bname,
                                ctx.arena.alloc(core::Term::Prim(Prim::U(phase))),
                            );
                            true
                        } else {
                            false
                        };

                        let (core_body, body_ty) = infer(ctx, phase, arm.body)?;

                        if pushed {
                            ctx.pop_local();
                        }

                        // All arms must agree on type.
                        match common_ty {
                            None => common_ty = Some(body_ty),
                            Some(ty) => {
                                if !types_equal(ty, body_ty) {
                                    return Err(anyhow!("match arms have different types"));
                                }
                            }
                        }

                        Ok(core::Arm {
                            pat: core_pat,
                            body: core_body,
                        })
                    }))?;

            let ty = common_ty.unwrap(); // arms is non-empty
            let core_term = ctx.alloc(core::Term::Match {
                scrutinee: core_scrutinee,
                arms: core_arms,
            });
            Ok((core_term, ty))
        }
    }
}

/// Elaborate a match pattern, returning the core pattern and any bound name
/// (arena-allocated so it lives in `'core`).
fn elaborate_pat<'src, 'core>(
    ctx: &Ctx<'core, '_>,
    pat: &'src ast::Pat<'src>,
) -> Result<(core::Pat<'core>, Option<&'core str>)> {
    match pat {
        ast::Pat::Lit(n) => Ok((core::Pat::Lit(*n), None)),
        ast::Pat::Name(name) => {
            let s = name.as_str();
            if s == "_" {
                Ok((core::Pat::Wildcard, None))
            } else {
                let bound: &'core str = ctx.arena.alloc_str(s);
                Ok((core::Pat::Bind(bound), Some(bound)))
            }
        }
    }
}

/// Elaborate a sequence of `let` bindings followed by a trailing expression (infer mode).
fn infer_block<'src, 'core>(
    ctx: &mut Ctx<'core, '_>,
    phase: Phase,
    stmts: &'src [ast::Let<'src>],
    expr: &'src ast::Term<'src>,
) -> Result<(&'core core::Term<'core>, &'core core::Term<'core>)> {
    match stmts {
        [] => infer(ctx, phase, expr),
        [first, rest @ ..] => {
            elaborate_let(ctx, phase, first, |ctx| infer_block(ctx, phase, rest, expr))
        }
    }
}

/// Elaborate a sequence of `let` bindings followed by a trailing expression (check mode).
fn check_block<'src, 'core>(
    ctx: &mut Ctx<'core, '_>,
    phase: Phase,
    stmts: &'src [ast::Let<'src>],
    expr: &'src ast::Term<'src>,
    expected: &'core core::Term<'core>,
) -> Result<&'core core::Term<'core>> {
    match stmts {
        [] => check(ctx, phase, expr, expected),
        [first, rest @ ..] => {
            let (let_term, _) = elaborate_let(ctx, phase, first, |ctx| {
                let core_body = check_block(ctx, phase, rest, expr, expected)?;
                // Wrap the body in a dummy type so elaborate_let can return a pair;
                // the type is unused by check_block's caller.
                Ok((core_body, expected))
            })?;
            Ok(let_term)
        }
    }
}

/// Elaborate a single `let` binding, then call `cont` with the extended context.
/// Returns `(Let { .. }, body_ty)` where `body_ty` is whatever `cont` returns.
fn elaborate_let<'src, 'core, F>(
    ctx: &mut Ctx<'core, '_>,
    phase: Phase,
    stmt: &'src ast::Let<'src>,
    cont: F,
) -> Result<(&'core core::Term<'core>, &'core core::Term<'core>)>
where
    F: FnOnce(&mut Ctx<'core, '_>) -> Result<(&'core core::Term<'core>, &'core core::Term<'core>)>,
{
    // Determine the binding type: use annotation if present, otherwise infer.
    let (core_expr, bind_ty) = if let Some(ann) = stmt.ty {
        let ty = elaborate_ty(ctx.arena, phase, ann)?;
        let core_e = check(ctx, phase, stmt.expr, ty)?;
        (core_e, ty)
    } else {
        infer(ctx, phase, stmt.expr)?
    };

    let bind_name: &'core str = ctx.arena.alloc_str(stmt.name.as_str());
    ctx.push_local(bind_name, bind_ty);
    let (core_body, body_ty) = cont(ctx)?;
    ctx.pop_local();

    let let_term = ctx.alloc(core::Term::Let {
        name: bind_name,
        ty: bind_ty,
        expr: core_expr,
        body: core_body,
    });
    Ok((let_term, body_ty))
}

/// Check `term` against `expected`, returning the elaborated core term.
pub fn check<'src, 'core>(
    ctx: &mut Ctx<'core, '_>,
    phase: Phase,
    term: &'src ast::Term<'src>,
    expected: &'core core::Term<'core>,
) -> Result<&'core core::Term<'core>> {
    match term {
        // ------------------------------------------------------------------ Lit
        // Literals check against any integer type.
        ast::Term::Lit(n) => match expected {
            core::Term::Prim(Prim::IntTy(_)) => Ok(ctx.alloc(core::Term::Lit(*n))),
            _ => Err(anyhow!("literal `{n}` cannot have a non-integer type")),
        },

        // ------------------------------------------------------------------ App { Prim (BinOp) }
        // Width is resolved from the expected type.
        ast::Term::App {
            func: ast::FunName::BinOp(op),
            args,
        } => {
            let int_ty = match expected {
                core::Term::Prim(Prim::IntTy(it)) => *it,
                _ => return Err(anyhow!("primitive operation requires an integer type")),
            };

            use ast::BinOp;
            let (prim, result_ty): (Prim, &'core core::Term<'core>) = match op {
                BinOp::Add => (Prim::Add(int_ty), expected),
                BinOp::Sub => (Prim::Sub(int_ty), expected),
                BinOp::Mul => (Prim::Mul(int_ty), expected),
                BinOp::Div => (Prim::Div(int_ty), expected),
                BinOp::BitAnd => (Prim::BitAnd(int_ty), expected),
                BinOp::BitOr => (Prim::BitOr(int_ty), expected),
                // Comparisons always return u1 regardless of the expected type.
                // But the expected type must be u1.
                BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => {
                    let u1 = IntType::new(IntWidth::U1, phase);
                    if int_ty.width != IntWidth::U1 {
                        return Err(anyhow!("comparison result type must be u1"));
                    }
                    let cmp_prim = match op {
                        BinOp::Eq => Prim::Eq(u1),
                        BinOp::Ne => Prim::Ne(u1),
                        BinOp::Lt => Prim::Lt(u1),
                        BinOp::Gt => Prim::Gt(u1),
                        BinOp::Le => Prim::Le(u1),
                        BinOp::Ge => Prim::Ge(u1),
                        _ => unreachable!(),
                    };
                    (cmp_prim, expected)
                }
            };

            // Comparisons take two operands of the *operand* type, not u1.
            // We need to figure out operand type for comparisons.
            let operand_ty: &'core core::Term<'core> = match op {
                BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => {
                    // The operand type is inferred from the arguments.
                    // We'll check two args and infer the width from the first.
                    if args.len() != 2 {
                        return Err(anyhow!("binary operation expects exactly 2 arguments"));
                    }
                    let (_, arg0_ty) = infer(ctx, phase, args[0])?;
                    arg0_ty
                }
                _ => expected,
            };

            if args.len() != 2 {
                return Err(anyhow!("binary operation expects exactly 2 arguments"));
            }

            let core_arg0 = check(ctx, phase, args[0], operand_ty)?;
            let core_arg1 = check(ctx, phase, args[1], operand_ty)?;

            // For comparisons, update prim to carry the operand width.
            let final_prim = match op {
                BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => {
                    match operand_ty {
                        core::Term::Prim(Prim::IntTy(op_int_ty)) => match op {
                            BinOp::Eq => Prim::Eq(*op_int_ty),
                            BinOp::Ne => Prim::Ne(*op_int_ty),
                            BinOp::Lt => Prim::Lt(*op_int_ty),
                            BinOp::Gt => Prim::Gt(*op_int_ty),
                            BinOp::Le => Prim::Le(*op_int_ty),
                            BinOp::Ge => Prim::Ge(*op_int_ty),
                            _ => unreachable!(),
                        },
                        _ => return Err(anyhow!("comparison operands must be integers")),
                    }
                }
                _ => prim,
            };

            let core_args = ctx.arena.alloc_slice_fill_iter([core_arg0, core_arg1]);
            let _ = result_ty; // already allocated as `expected`
            Ok(ctx.alloc(core::Term::App {
                head: core::Head::Prim(final_prim),
                args: core_args,
            }))
        }

        // ------------------------------------------------------------------ App { UnOp }
        ast::Term::App {
            func: ast::FunName::UnOp(op),
            args,
        } => {
            let int_ty = match expected {
                core::Term::Prim(Prim::IntTy(it)) => *it,
                _ => return Err(anyhow!("primitive operation requires an integer type")),
            };

            use ast::UnOp;
            let prim = match op {
                UnOp::Not => Prim::BitNot(int_ty),
            };

            if args.len() != 1 {
                return Err(anyhow!("unary operation expects exactly 1 argument"));
            }
            let core_arg = check(ctx, phase, args[0], expected)?;
            let core_args = ctx.arena.alloc_slice_fill_iter([core_arg]);
            Ok(ctx.alloc(core::Term::App {
                head: core::Head::Prim(prim),
                args: core_args,
            }))
        }

        // ------------------------------------------------------------------ Quote (check mode)
        // `#(t)` checked against `[[T]]` — check `t` against `T` at object phase.
        ast::Term::Quote(inner) => match expected {
            core::Term::Lift(obj_ty) => {
                let core_inner = check(ctx, Phase::Object, inner, obj_ty)?;
                Ok(ctx.alloc(core::Term::Quote(core_inner)))
            }
            _ => Err(anyhow!("quote `#(...)` must have a lifted type `[[T]]`")),
        },

        // ------------------------------------------------------------------ Block (check mode)
        // Thread the expected type down through let-bindings to the final expression.
        ast::Term::Block { stmts, expr } => {
            let depth_before = ctx.depth();
            let result = check_block(ctx, phase, stmts, expr, expected);
            while ctx.depth() > depth_before {
                ctx.pop_local();
            }
            result
        }

        // ------------------------------------------------------------------ fallthrough: infer then unify
        // For all other forms, infer the type and check it matches expected.
        _ => {
            let (core_term, inferred_ty) = infer(ctx, phase, term)?;
            if !types_equal(inferred_ty, expected) {
                return Err(anyhow!("type mismatch"));
            }
            Ok(core_term)
        }
    }
}

#[cfg(test)]
mod test;
