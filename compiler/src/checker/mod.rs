use std::collections::HashMap;

use anyhow::{Context as _, Result, anyhow, bail, ensure};

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
    globals: &'globals HashMap<core::Name<'core>, core::FunSig<'core>>,
}

impl<'core, 'globals> Ctx<'core, 'globals> {
    pub const fn new(
        arena: &'core bumpalo::Bump,
        globals: &'globals HashMap<core::Name<'core>, core::FunSig<'core>>,
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
    const fn depth(&self) -> usize {
        self.locals.len()
    }

    /// Helper to create a lifted type [[T]]
    pub fn lift_ty(&self, inner: &'core core::Term<'core>) -> &'core core::Term<'core> {
        self.arena.alloc(core::Term::Lift(inner))
    }

    /// Recover the type of an already-elaborated core term without re-elaborating.
    ///
    /// Precondition: `term` was produced by `infer` or `check` in a context
    /// compatible with `self`.  Panics on typechecker invariant violations.
    pub fn type_of(&mut self, term: &'core core::Term<'core>) -> &'core core::Term<'core> {
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
            ) => core::Term::int_ty(it.width, it.phase),

            // Variable: look up by De Bruijn level.
            core::Term::Var(lvl) => {
                self.locals
                    .get(lvl.0)
                    .expect("Var level out of range (typechecker invariant)")
                    .1
            }

            // Primitive types inhabit the relevant universe.
            core::Term::Prim(Prim::IntTy(it)) => core::Term::universe(it.phase),
            // Type, VmType, and [[T]] all inhabit Type (meta universe).
            core::Term::Prim(Prim::U(_)) | core::Term::Lift(_) => &core::Term::TYPE,

            // Comparison ops return u1 at the operand phase.
            core::Term::Prim(
                Prim::Eq(it)
                | Prim::Ne(it)
                | Prim::Lt(it)
                | Prim::Gt(it)
                | Prim::Le(it)
                | Prim::Ge(it),
            ) => core::Term::u1_ty(it.phase),

            // Embed: IntTy(w, Meta) -> [[IntTy(w, Object)]]
            core::Term::Prim(Prim::Embed(w)) => {
                self.alloc(core::Term::Lift(core::Term::int_ty(*w, Phase::Object)))
            }

            // Application: return type comes from the head.
            core::Term::App(app) => match &app.head {
                core::Head::Global(name) => {
                    self.globals
                        .get(name)
                        .expect("App/Global with unknown name (typechecker invariant)")
                        .ret_ty
                }
                core::Head::Prim(p) => match *p {
                    Prim::Add(it)
                    | Prim::Sub(it)
                    | Prim::Mul(it)
                    | Prim::Div(it)
                    | Prim::BitAnd(it)
                    | Prim::BitOr(it)
                    | Prim::BitNot(it) => core::Term::int_ty(it.width, it.phase),
                    Prim::Eq(it)
                    | Prim::Ne(it)
                    | Prim::Lt(it)
                    | Prim::Gt(it)
                    | Prim::Le(it)
                    | Prim::Ge(it) => core::Term::u1_ty(it.phase),
                    Prim::Embed(w) => {
                        self.alloc(core::Term::Lift(core::Term::int_ty(w, Phase::Object)))
                    }
                    Prim::IntTy(_) | Prim::U(_) => {
                        unreachable!("type-level prim in App head (typechecker invariant)")
                    }
                },
            },

            // #(t) : [[type_of(t)]]
            core::Term::Quote(inner) => {
                let inner_ty = self.type_of(inner);
                self.alloc(core::Term::Lift(inner_ty))
            }

            // $(t) where t : [[T]] — strips the Lift.
            core::Term::Splice(inner) => {
                let inner_ty = self.type_of(inner);
                match inner_ty {
                    core::Term::Lift(object_ty) => object_ty,
                    core::Term::Var(_)
                    | core::Term::Prim(_)
                    | core::Term::Lit(..)
                    | core::Term::App(_)
                    | core::Term::Quote(_)
                    | core::Term::Splice(_)
                    | core::Term::Let(_)
                    | core::Term::Match(_) => {
                        unreachable!("Splice inner must have Lift type (typechecker invariant)")
                    }
                }
            }

            // let x : T = e in body — type is type_of(body) with x in scope.
            core::Term::Let(core::Let { name, ty, body, .. }) => {
                self.push_local(name, ty);
                let result = self.type_of(body);
                self.pop_local();
                result
            }

            // match: all arms share the same type; recover from the first.
            core::Term::Match(core::Match { scrutinee, arms }) => {
                let arm = arms
                    .first()
                    .expect("Match with no arms (typechecker invariant)");
                match arm.pat {
                    core::Pat::Lit(_) | core::Pat::Wildcard => self.type_of(arm.body),
                    core::Pat::Bind(name) => {
                        let scrut_ty = self.type_of(scrutinee);
                        self.push_local(name, scrut_ty);
                        let result = self.type_of(arm.body);
                        self.pop_local();
                        result
                    }
                }
            }
        }
    }
}

/// Resolve a built-in type name to a static core term, using `phase` for integer types.
///
/// Returns `None` if the name is not a built-in type.
fn builtin_prim_ty(name: &str, phase: Phase) -> Option<&'static core::Term<'static>> {
    Some(match name {
        "u1" => core::Term::int_ty(IntWidth::U1, phase),
        "u8" => core::Term::int_ty(IntWidth::U8, phase),
        "u16" => core::Term::int_ty(IntWidth::U16, phase),
        "u32" => core::Term::int_ty(IntWidth::U32, phase),
        "u64" => core::Term::int_ty(IntWidth::U64, phase),
        "Type" => &core::Term::TYPE,
        "VmType" => &core::Term::VM_TYPE,
        _ => return None,
    })
}

fn elaborate_sig<'src, 'core>(
    arena: &'core bumpalo::Bump,
    func: &ast::Function<'src>,
) -> Result<core::FunSig<'core>> {
    let empty_globals = HashMap::new();
    let mut ctx = Ctx::new(arena, &empty_globals);

    let params: &'core [(&'core str, &'core core::Term<'core>)] =
        arena.alloc_slice_try_fill_iter(func.params.iter().map(|p| -> Result<_> {
            let param_name: &'core str = arena.alloc_str(p.name.as_str());
            let param_ty = infer(&mut ctx, func.phase, p.ty)?;
            Ok((param_name, param_ty))
        }))?;

    let ret_ty = infer(&mut ctx, func.phase, func.ret_ty)?;

    Ok(core::FunSig {
        params,
        ret_ty,
        phase: func.phase,
    })
}

/// Pass 1: collect all top-level function signatures into a globals table.
///
/// Type annotations on parameters and return types are elaborated here so that
/// pass 2 (body elaboration) has fully-typed signatures available for all
/// functions, including forward references.
pub(crate) fn collect_signatures<'src, 'core>(
    arena: &'core bumpalo::Bump,
    program: &ast::Program<'src>,
) -> Result<HashMap<core::Name<'core>, core::FunSig<'core>>> {
    let mut globals: HashMap<core::Name<'core>, core::FunSig<'core>> = HashMap::new();

    for func in program.functions {
        let name = core::Name::new(arena.alloc_str(func.name.as_str()));

        ensure!(
            !globals.contains_key(&name),
            "duplicate function name `{name}`"
        );

        let sig = elaborate_sig(arena, func).with_context(|| format!("in function `{name}`"))?;

        globals.insert(name, sig);
    }

    Ok(globals)
}

/// Pass 2: elaborate all function bodies with the full globals table available.
fn elaborate_bodies<'src, 'core>(
    arena: &'core bumpalo::Bump,
    program: &ast::Program<'src>,
    globals: &HashMap<core::Name<'core>, core::FunSig<'core>>,
) -> Result<core::Program<'core>> {
    let functions: &'core [core::Function<'core>] =
        arena.alloc_slice_try_fill_iter(program.functions.iter().map(|func| -> Result<_> {
            let name = core::Name::new(arena.alloc_str(func.name.as_str()));
            let ast_sig = globals.get(&name).expect("signature missing from pass 1");

            // Build a fresh context borrowing the stack-owned globals map.
            let mut ctx = Ctx::new(arena, globals);

            // Push parameters as locals so the body can reference them.
            for (pname, pty) in ast_sig.params {
                ctx.push_local(pname, pty);
            }

            // Elaborate the body, checking it against the declared return type.
            let body = check(&mut ctx, ast_sig.phase, func.body, ast_sig.ret_ty)
                .with_context(|| format!("in function `{name}`"))?;

            // Re-borrow sig from globals (ctx was consumed in the check above).
            // We need the sig fields for the Function; collect them before moving ctx.
            let sig = core::FunSig {
                params: ast_sig.params,
                ret_ty: ast_sig.ret_ty,
                phase: ast_sig.phase,
            };

            Ok(core::Function { name, sig, body })
        }))?;

    Ok(core::Program { functions })
}

/// Elaborate the entire program in two passes
pub fn elaborate_program<'core>(
    arena: &'core bumpalo::Bump,
    program: &ast::Program<'_>,
) -> Result<core::Program<'core>> {
    let globals = collect_signatures(arena, program)?;
    elaborate_bodies(arena, program, &globals)
}

/// Return the universe phase that `ty` inhabits, or `None` if it cannot be determined.
///
/// This is the core analogue of the 2LTT kinding judgement:
///   - `IntTy(_, p)` inhabits `U(p)`
///   - `U(Meta)` (Type) inhabits `U(Meta)`   (type-in-type for the meta universe)
///   - `U(Object)` (`VmType`) inhabits `U(Meta)` (the meta universe classifies object types)
///   - `Lift(_)` inhabits `U(Meta)`
const fn type_universe(ty: &core::Term<'_>) -> Option<Phase> {
    match ty {
        core::Term::Prim(Prim::IntTy(IntType { phase, .. })) => Some(*phase),
        core::Term::Prim(Prim::U(_)) | core::Term::Lift(_) => Some(Phase::Meta),
        core::Term::Var(_)
        | core::Term::Prim(_)
        | core::Term::Lit(..)
        | core::Term::App { .. }
        | core::Term::Quote(_)
        | core::Term::Splice(_)
        | core::Term::Let { .. }
        | core::Term::Match { .. } => None,
    }
}

/// Structural equality of core types (no normalisation needed for this prototype).
fn types_equal(a: &core::Term<'_>, b: &core::Term<'_>) -> bool {
    // Uses pointer equality as a fast path — terms allocated from the same arena
    // slot are guaranteed identical without recursion.
    std::ptr::eq(a, b) || a == b
}

/// Synthesise and return the elaborated core term; recover its type via `ctx.type_of`.
pub fn infer<'src, 'core>(
    ctx: &mut Ctx<'core, '_>,
    phase: Phase,
    term: &'src ast::Term<'src>,
) -> Result<&'core core::Term<'core>> {
    match term {
        // ------------------------------------------------------------------ Var
        // Look up the name in locals; return its level and type.
        ast::Term::Var(name) => {
            let name_str = name.as_str();
            // First check if it's a built-in type name — those are inferable too.
            if let Some(term) = builtin_prim_ty(name_str, phase) {
                // Phase check: U(Object) (VmType) is only valid in a meta-phase context.
                if let core::Term::Prim(Prim::U(u_phase)) = term {
                    ensure!(
                        *u_phase == phase,
                        "`{name_str}` is a {u_phase}-phase type, \
                         not valid in a {phase}-phase context"
                    );
                }
                return Ok(term);
            }
            // Otherwise look in locals.
            let (lvl, _) = ctx
                .lookup_local(name_str)
                .ok_or_else(|| anyhow!("unbound variable `{name_str}`"))?;
            Ok(ctx.alloc(core::Term::Var(lvl)))
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
            let sig = ctx
                .globals
                .get(name)
                .ok_or_else(|| anyhow!("unknown function `{name}`"))?;

            // The call phase must match the current elaboration phase.
            let call_phase = sig.phase;
            ensure!(
                call_phase == phase,
                "function `{name}` is a {call_phase}-phase function, but called in {phase}-phase context"
            );
            let params = sig.params;

            ensure!(
                args.len() == params.len(),
                "function `{name}` expects {} argument(s), got {}",
                params.len(),
                args.len()
            );

            // Check each argument against its declared parameter type.
            let core_args: &'core [&'core core::Term<'core>] = ctx
                .arena
                .alloc_slice_try_fill_iter(args.iter().zip(params.iter()).map(
                    |(arg, (pname, pty))| -> Result<_> {
                        let core_arg = check(ctx, call_phase, arg, pty)
                            .with_context(|| format!("in call to '{name}' argument '{pname}'"))?;
                        Ok(core_arg)
                    },
                ))?;

            Ok(ctx.alloc(core::Term::new_app(
                core::Head::Global(core::Name::new(ctx.arena.alloc_str(name.as_str()))),
                core_args,
            )))
        }

        // ------------------------------------------------------------------ App { Prim (BinOp/UnOp) }
        // Arithmetic/bitwise ops are check-only (width comes from expected type).
        // Comparison ops are inferable: they always return u1, and the operand type
        // is inferred from the first argument (the second is checked to match).
        ast::Term::App {
            func: ast::FunName::BinOp(op),
            args,
        } if matches!(
            op,
            ast::BinOp::Eq
                | ast::BinOp::Ne
                | ast::BinOp::Lt
                | ast::BinOp::Gt
                | ast::BinOp::Le
                | ast::BinOp::Ge
        ) =>
        {
            use ast::BinOp;
            let [lhs, rhs] = args else {
                bail!("binary operation expects exactly 2 arguments")
            };

            // Infer the operand type from the first argument.
            let core_arg0 = infer(ctx, phase, lhs)?;
            let operand_ty = ctx.type_of(core_arg0);
            // Check the second argument against the same operand type.
            let core_arg1 = check(ctx, phase, rhs, operand_ty)?;
            // Verify both operands are integers and build the prim carrying the operand type.
            let op_int_ty = match operand_ty {
                core::Term::Prim(Prim::IntTy(it)) => *it,
                core::Term::Var(_)
                | core::Term::Prim(_)
                | core::Term::Lit(..)
                | core::Term::App(_)
                | core::Term::Lift(_)
                | core::Term::Quote(_)
                | core::Term::Splice(_)
                | core::Term::Let(_)
                | core::Term::Match(_) => {
                    ensure!(false, "comparison operands must be integers");
                    unreachable!()
                }
            };
            let prim = match op {
                BinOp::Eq => Prim::Eq(op_int_ty),
                BinOp::Ne => Prim::Ne(op_int_ty),
                BinOp::Lt => Prim::Lt(op_int_ty),
                BinOp::Gt => Prim::Gt(op_int_ty),
                BinOp::Le => Prim::Le(op_int_ty),
                BinOp::Ge => Prim::Ge(op_int_ty),
                BinOp::Add
                | BinOp::Sub
                | BinOp::Mul
                | BinOp::Div
                | BinOp::BitAnd
                | BinOp::BitOr => unreachable!(),
            };
            let core_args = ctx.alloc_slice([core_arg0, core_arg1]);
            Ok(ctx.alloc(core::Term::new_app(core::Head::Prim(prim), core_args)))
        }
        ast::Term::App {
            func: ast::FunName::BinOp(_) | ast::FunName::UnOp(_),
            ..
        } => Err(anyhow!(
            "cannot infer type of a primitive operation; add a type annotation"
        )),

        // ------------------------------------------------------------------ Lift
        // `[[T]]` — elaborate T at the object phase, type is Type (meta universe).
        ast::Term::Lift(inner) => {
            // Lift is only legal in meta phase.
            ensure!(
                phase == Phase::Meta,
                "`[[...]]` is only valid in a meta-phase context"
            );
            // The inner expression must be an object type.
            let core_inner = infer(ctx, Phase::Object, inner)?;
            // Verify the inner term is indeed a type (inhabits VmType).
            ensure!(
                types_equal(ctx.type_of(core_inner), &core::Term::VM_TYPE),
                "argument of `[[...]]` must be an object type"
            );
            Ok(ctx.alloc(core::Term::Lift(core_inner)))
        }

        // ------------------------------------------------------------------ Quote
        // `#(t)` — infer iff the inner term is inferable (phase shifts meta→object).
        ast::Term::Quote(inner) => {
            // Quote is only legal in meta phase.
            ensure!(
                phase == Phase::Meta,
                "`#(...)` is only valid in a meta-phase context"
            );
            let core_inner = infer(ctx, Phase::Object, inner)?;
            Ok(ctx.alloc(core::Term::Quote(core_inner)))
        }

        // ------------------------------------------------------------------ Splice
        // `$(t)` — infer iff `t` infers as `[[T]]`; result type is `T` (phase shifts object→meta).
        // If `t` infers as a meta integer `IntTy(w, Meta)`, insert an implicit `Embed(w)`
        // to produce `[[IntTy(w, Object)]]` before splicing.
        ast::Term::Splice(inner) => {
            // Splice is only legal in object phase.
            ensure!(
                phase == Phase::Object,
                "`$(...)` is only valid in an object-phase context"
            );
            let core_inner = infer(ctx, Phase::Meta, inner)?;
            let inner_ty = ctx.type_of(core_inner);
            match inner_ty {
                core::Term::Lift(_) => Ok(ctx.alloc(core::Term::Splice(core_inner))),
                // A meta-level integer is implicitly embedded: insert Embed(w) so that
                // the splice argument has type `[[IntTy(w, Object)]]`.
                core::Term::Prim(Prim::IntTy(IntType {
                    width,
                    phase: Phase::Meta,
                })) => {
                    let embedded = ctx.alloc(core::Term::new_app(
                        core::Head::Prim(Prim::Embed(*width)),
                        ctx.alloc_slice([core_inner]),
                    ));
                    Ok(ctx.alloc(core::Term::Splice(embedded)))
                }
                core::Term::Var(_)
                | core::Term::Prim(_)
                | core::Term::Lit(..)
                | core::Term::App(_)
                | core::Term::Quote(_)
                | core::Term::Splice(_)
                | core::Term::Let(_)
                | core::Term::Match(_) => Err(anyhow!(
                    "argument of `$(...)` must have a lifted type `[[T]]` or be a meta-level integer"
                )),
            }
        }

        // ------------------------------------------------------------------ Block (Let*)
        // Elaborate each `let` binding in sequence, then the trailing expression.
        ast::Term::Block { stmts, expr } => {
            let depth_before = ctx.depth();
            let result = infer_block(ctx, phase, stmts, expr);
            // Each let-binding is responsible for pushing and popping its own local
            // (via `elaborate_let`), so the depth must be restored exactly.
            assert_eq!(ctx.depth(), depth_before, "infer_block leaked locals");
            result
        }

        // ------------------------------------------------------------------ Match
        // Without an expected type, match is not inferable — require an annotation.
        ast::Term::Match { .. } => Err(anyhow!(
            "cannot infer type of match expression; add a type annotation or use in a \
             checked position"
        )),
    }
}

/// Check exhaustiveness of `arms` given the scrutinee type `scrut_ty`.
///
/// Returns `Err` if coverage cannot be established.
fn check_exhaustiveness(scrut_ty: &core::Term<'_>, arms: &[ast::MatchArm<'_>]) -> Result<()> {
    // For u0/u1/u8 scrutinees we track which literal values have been covered
    // using a Vec<bool> of length 1/2/256 respectively.  If all entries become
    // true the match is exhaustive even without a wildcard.  For any other type
    // (u16/u32/u64) we only accept a wildcard or bind-all arm as evidence of
    // exhaustiveness, since enumerating every value is impractical.
    let mut covered_lits: Option<Vec<bool>> = match scrut_ty {
        core::Term::Prim(Prim::IntTy(ty)) => match ty.width {
            IntWidth::U0 => Some(vec![false; 1]),
            IntWidth::U1 => Some(vec![false; 2]),
            IntWidth::U8 => Some(vec![false; 256]),
            IntWidth::U16 | IntWidth::U32 | IntWidth::U64 => None,
        },
        core::Term::Var(_)
        | core::Term::Prim(_)
        | core::Term::Lit(..)
        | core::Term::App { .. }
        | core::Term::Lift(_)
        | core::Term::Quote(_)
        | core::Term::Splice(_)
        | core::Term::Let { .. }
        | core::Term::Match { .. } => None,
    };
    let mut has_catch_all = false;

    for arm in arms {
        match &arm.pat {
            ast::Pat::Name(_) => {
                has_catch_all = true;
            }
            ast::Pat::Lit(n) => {
                if let Some(ref mut bits) = covered_lits {
                    let bit = bits
                        .get_mut(usize::try_from(*n)?)
                        .context("Pattern literal out of range")?;
                    *bit = true;
                }
            }
        }
    }

    let fully_covered = covered_lits.is_some_and(|bits| bits.iter().all(|&b| b));
    ensure!(
        has_catch_all || fully_covered,
        "match expression is not exhaustive: no wildcard or bind-all arm"
    );
    Ok(())
}

/// Elaborate a match pattern into a core pattern.
/// Any bound name can be recovered via `core::Pat::bound_name()`.
fn elaborate_pat<'core>(ctx: &Ctx<'core, '_>, pat: &ast::Pat<'_>) -> core::Pat<'core> {
    match pat {
        ast::Pat::Lit(n) => core::Pat::Lit(*n),
        ast::Pat::Name(name) => {
            let s = name.as_str();
            if s == "_" {
                core::Pat::Wildcard
            } else {
                let bound: &'core str = ctx.arena.alloc_str(s);
                core::Pat::Bind(bound)
            }
        }
    }
}

/// Elaborate a single `let` binding: resolve the binding type, elaborate the
/// initialiser, push the local into the context, call `cont`, then pop and
/// assemble `core::Term::Let`.
///
/// `cont` receives the extended context and returns any result `T`.  A
/// `body_of` accessor is used to extract the body term (needed to build the
/// `Let` node) from `T`, and a `wrap` function replaces the body in `T` with
/// the finished `Let` node — letting the caller thread arbitrary extra data
/// (e.g. the inferred type) through without any dummy pairs.
fn elaborate_let<'src, 'core, T, F, G, W>(
    ctx: &mut Ctx<'core, '_>,
    phase: Phase,
    stmt: &'src ast::Let<'src>,
    cont: F,
    body_of: G,
    wrap: W,
) -> Result<T>
where
    F: FnOnce(&mut Ctx<'core, '_>) -> Result<T>,
    G: FnOnce(&T) -> &'core core::Term<'core>,
    W: FnOnce(&'core core::Term<'core>, T) -> T,
{
    // Determine the binding type: use annotation if present, otherwise infer.
    let (core_expr, bind_ty) = if let Some(ann) = stmt.ty {
        let ty = infer(ctx, phase, ann)?;
        let core_e = check(ctx, phase, stmt.expr, ty)
            .with_context(|| format!("in let binding `{}`", stmt.name.as_str()))?;
        (core_e, ty)
    } else {
        let core_e = infer(ctx, phase, stmt.expr)
            .with_context(|| format!("in let binding `{}`", stmt.name.as_str()))?;
        let bind_ty = ctx.type_of(core_e);
        (core_e, bind_ty)
    };

    let bind_name: &'core str = ctx.arena.alloc_str(stmt.name.as_str());
    ctx.push_local(bind_name, bind_ty);
    let cont_result = cont(ctx);
    ctx.pop_local();
    let cont_result = cont_result?;

    let core_body = body_of(&cont_result);
    let let_term = ctx.alloc(core::Term::new_let(
        bind_name, bind_ty, core_expr, core_body,
    ));
    Ok(wrap(let_term, cont_result))
}

/// Elaborate a sequence of `let` bindings followed by a trailing expression (infer mode).
fn infer_block<'src, 'core>(
    ctx: &mut Ctx<'core, '_>,
    phase: Phase,
    stmts: &'src [ast::Let<'src>],
    expr: &'src ast::Term<'src>,
) -> Result<&'core core::Term<'core>> {
    match stmts {
        [] => infer(ctx, phase, expr),
        [first, rest @ ..] => elaborate_let(
            ctx,
            phase,
            first,
            |ctx| infer_block(ctx, phase, rest, expr),
            |body| body,
            |let_term, _body| let_term,
        ),
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
        [first, rest @ ..] => elaborate_let(
            ctx,
            phase,
            first,
            |ctx| check_block(ctx, phase, rest, expr, expected),
            |body| body,
            |let_term, _body| let_term,
        ),
    }
}

/// Check `term` against `expected`, returning the elaborated core term.
pub fn check<'src, 'core>(
    ctx: &mut Ctx<'core, '_>,
    phase: Phase,
    term: &'src ast::Term<'src>,
    expected: &'core core::Term<'core>,
) -> Result<&'core core::Term<'core>> {
    // Verify `expected` inhabits the correct universe for the current phase.
    // Every `expected` originates from `elaborate_ty` or from `infer`, both of which
    // only produce `IntTy`, `U`, or `Lift` — so `None` here is an internal compiler bug.
    let ty_phase = type_universe(expected)
        .expect("expected type passed to `check` is not a well-formed type expression");
    ensure!(
        ty_phase == phase,
        "expected type inhabits the {ty_phase}-phase universe, \
         but elaborating at {phase} phase"
    );
    match term {
        // ------------------------------------------------------------------ Lit
        // Literals check against any integer type.
        ast::Term::Lit(n) => match expected {
            core::Term::Prim(Prim::IntTy(it)) => {
                let width = it.width;
                ensure!(
                    *n <= width.max_value(),
                    "literal `{n}` does not fit in type `{width}`"
                );
                Ok(ctx.alloc(core::Term::Lit(*n, *it)))
            }
            core::Term::Var(_)
            | core::Term::Prim(_)
            | core::Term::Lit(..)
            | core::Term::App { .. }
            | core::Term::Lift(_)
            | core::Term::Quote(_)
            | core::Term::Splice(_)
            | core::Term::Let { .. }
            | core::Term::Match { .. } => {
                Err(anyhow!("literal `{n}` cannot have a non-integer type"))
            }
        },

        // ------------------------------------------------------------------ App { Prim (BinOp) }
        // Width is resolved from the expected type.
        // Comparison ops (Eq/Ne/Lt/Gt/Le/Ge) are handled in infer mode and fall through
        // to infer+unify below, since they always return u1 (inferable).
        ast::Term::App {
            func: ast::FunName::BinOp(op),
            args,
        } if !matches!(
            op,
            ast::BinOp::Eq
                | ast::BinOp::Ne
                | ast::BinOp::Lt
                | ast::BinOp::Gt
                | ast::BinOp::Le
                | ast::BinOp::Ge
        ) =>
        {
            let int_ty = match expected {
                core::Term::Prim(Prim::IntTy(it)) => *it,
                core::Term::Var(_)
                | core::Term::Prim(_)
                | core::Term::Lit(..)
                | core::Term::App { .. }
                | core::Term::Lift(_)
                | core::Term::Quote(_)
                | core::Term::Splice(_)
                | core::Term::Let { .. }
                | core::Term::Match { .. } => {
                    bail!("primitive operation requires an integer type")
                }
            };

            use ast::BinOp;
            let prim = match op {
                BinOp::Add => Prim::Add(int_ty),
                BinOp::Sub => Prim::Sub(int_ty),
                BinOp::Mul => Prim::Mul(int_ty),
                BinOp::Div => Prim::Div(int_ty),
                BinOp::BitAnd => Prim::BitAnd(int_ty),
                BinOp::BitOr => Prim::BitOr(int_ty),
                BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => {
                    unreachable!("comparisons are excluded by guard")
                }
            };

            let [lhs, rhs] = args else {
                bail!("binary operation expects exactly 2 arguments")
            };

            let core_arg0 = check(ctx, phase, lhs, expected)?;
            let core_arg1 = check(ctx, phase, rhs, expected)?;

            let core_args = ctx.alloc_slice([core_arg0, core_arg1]);
            Ok(ctx.alloc(core::Term::new_app(core::Head::Prim(prim), core_args)))
        }

        // ------------------------------------------------------------------ App { UnOp }
        ast::Term::App {
            func: ast::FunName::UnOp(op),
            args,
        } => {
            let int_ty = match expected {
                core::Term::Prim(Prim::IntTy(it)) => *it,
                core::Term::Var(_)
                | core::Term::Prim(_)
                | core::Term::Lit(..)
                | core::Term::App(_)
                | core::Term::Lift(_)
                | core::Term::Quote(_)
                | core::Term::Splice(_)
                | core::Term::Let(_)
                | core::Term::Match(_) => {
                    bail!("primitive operation requires an integer type")
                }
            };

            let prim = match op {
                ast::UnOp::Not => Prim::BitNot(int_ty),
            };

            let [arg] = args else {
                bail!("unary operation expects exactly 1 argument")
            };
            let core_arg = check(ctx, phase, arg, expected)?;
            let core_args = std::slice::from_ref(ctx.arena.alloc(core_arg));
            Ok(ctx.alloc(core::Term::new_app(core::Head::Prim(prim), core_args)))
        }

        // ------------------------------------------------------------------ Quote (check mode)
        // `#(t)` checked against `[[T]]` — check `t` against `T` at object phase.
        ast::Term::Quote(inner) => match expected {
            core::Term::Lift(obj_ty) => {
                let core_inner = check(ctx, Phase::Object, inner, obj_ty)?;
                Ok(ctx.alloc(core::Term::Quote(core_inner)))
            }
            core::Term::Var(_)
            | core::Term::Prim(_)
            | core::Term::Lit(..)
            | core::Term::App(_)
            | core::Term::Quote(_)
            | core::Term::Splice(_)
            | core::Term::Let(_)
            | core::Term::Match(_) => {
                Err(anyhow!("quote `#(...)` must have a lifted type `[[T]]`"))
            }
        },

        // ------------------------------------------------------------------ Splice (check mode)
        // `$(e)` checked against `T` (object) — check `e` against `[[T]]` at meta phase.
        // Mirror image of Quote: Quote unwraps `[[T]]` to check inner at object phase;
        // Splice wraps `T` in `[[...]]` to check inner at meta phase.
        //
        // For object integer types `T = IntTy(w, Object)`, also accept `e : IntTy(w, Meta)`
        // with an implicit `Embed(w)` insertion — the same coercion as the infer path.
        ast::Term::Splice(inner) => {
            ensure!(
                phase == Phase::Object,
                "`$(...)` is only valid in an object-phase context"
            );
            // For object integer expected types, first try the standard [[T]] path; if
            // that fails, try the meta-integer embed path (inner has type IntTy(w, Meta)).
            // Trying [[T]] first means a variable `x : [[u64]]` is always handled
            // correctly and the embed path only activates when [[T]] genuinely fails.
            if let core::Term::Prim(Prim::IntTy(IntType {
                width,
                phase: Phase::Object,
            })) = expected
            {
                let lift_ty = ctx.alloc(core::Term::Lift(expected));
                if let Ok(core_inner) = check(ctx, Phase::Meta, inner, lift_ty) {
                    return Ok(ctx.alloc(core::Term::Splice(core_inner)));
                }
                let meta_int_ty = ctx.alloc(core::Term::Prim(Prim::IntTy(IntType::meta(*width))));
                let core_inner = check(ctx, Phase::Meta, inner, meta_int_ty)?;
                let embedded = ctx.alloc(core::Term::new_app(
                    core::Head::Prim(Prim::Embed(*width)),
                    ctx.arena.alloc_slice_fill_iter([core_inner]),
                ));
                return Ok(ctx.alloc(core::Term::Splice(embedded)));
            }
            let lift_ty = ctx.alloc(core::Term::Lift(expected));
            let core_inner = check(ctx, Phase::Meta, inner, lift_ty)?;
            Ok(ctx.alloc(core::Term::Splice(core_inner)))
        }

        // ------------------------------------------------------------------ Match (check mode)
        // Check each arm body against the expected type; the scrutinee is always inferred.
        ast::Term::Match { scrutinee, arms } => {
            let core_scrutinee = infer(ctx, phase, scrutinee)?;
            let scrut_ty = ctx.type_of(core_scrutinee);

            check_exhaustiveness(scrut_ty, arms)?;

            let core_arms: &'core [core::Arm<'core>] =
                ctx.arena
                    .alloc_slice_try_fill_iter(arms.iter().map(|arm| -> Result<_> {
                        let core_pat = elaborate_pat(ctx, &arm.pat);
                        // If the pattern binds a name, push it into locals for the arm body.
                        // We use a placeholder type (scrutinee type) — sufficient for the prototype.
                        if let Some(bname) = core_pat.bound_name() {
                            ctx.push_local(bname, scrut_ty);
                        }

                        let arm_result = check(ctx, phase, arm.body, expected);

                        if core_pat.bound_name().is_some() {
                            ctx.pop_local();
                        }

                        let core_body = arm_result?;
                        Ok(core::Arm {
                            pat: core_pat,
                            body: core_body,
                        })
                    }))?;

            Ok(ctx.alloc(core::Term::new_match(core_scrutinee, core_arms)))
        }

        // ------------------------------------------------------------------ Block (check mode)
        // Thread the expected type down through let-bindings to the final expression.
        ast::Term::Block { stmts, expr } => {
            let depth_before = ctx.depth();
            let result = check_block(ctx, phase, stmts, expr, expected);
            // Each let-binding is responsible for pushing and popping its own local
            // (via `elaborate_let`), so the depth must be restored exactly.
            assert_eq!(ctx.depth(), depth_before, "check_block leaked locals");
            result
        }

        // ------------------------------------------------------------------ fallthrough: infer then unify
        // For all other forms, infer the type and check it matches expected.
        ast::Term::Var(_) | ast::Term::App { .. } | ast::Term::Lift(_) => {
            let core_term = infer(ctx, phase, term)?;
            ensure!(
                types_equal(ctx.type_of(core_term), expected),
                "type mismatch"
            );
            Ok(core_term)
        }
    }
}

#[cfg(test)]
mod test;
