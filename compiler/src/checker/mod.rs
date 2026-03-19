use std::collections::HashMap;

use anyhow::{anyhow, Context as _, Result};

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

    /// Helper to create an integer type term at the given phase
    pub fn int_ty(&self, width: IntWidth, phase: Phase) -> &'core core::Term<'core> {
        self.arena
            .alloc(core::Term::Prim(Prim::IntTy(IntType::new(width, phase))))
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

    /// Helper to create a `VmType` (object universe) term
    pub fn vm_type_ty(&self) -> &'core core::Term<'core> {
        self.arena.alloc(core::Term::Prim(Prim::U(Phase::Object)))
    }

    /// Helper to create a lifted type [[T]]
    pub fn lift_ty(&self, inner: &'core core::Term<'core>) -> &'core core::Term<'core> {
        self.arena.alloc(core::Term::Lift(inner))
    }
}

/// Resolve a built-in name to a `Prim`, using `phase` for integer types.
///
/// Returns `None` if the name is not a built-in.
fn builtin_prim(name: &str, phase: Phase) -> Option<Prim> {
    Some(match name {
        "u1" => Prim::IntTy(IntType::new(IntWidth::U1, phase)),
        "u8" => Prim::IntTy(IntType::new(IntWidth::U8, phase)),
        "u16" => Prim::IntTy(IntType::new(IntWidth::U16, phase)),
        "u32" => Prim::IntTy(IntType::new(IntWidth::U32, phase)),
        "u64" => Prim::IntTy(IntType::new(IntWidth::U64, phase)),
        "Type" => Prim::U(Phase::Meta),
        "VmType" => Prim::U(Phase::Object),
        _ => return None,
    })
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
            let prim = builtin_prim(name.as_str(), phase)
                .ok_or_else(|| anyhow!("unknown type `{}`", name.as_str()))?;
            // Verify the resulting type inhabits the correct universe for `phase`.
            // `Type` always inhabits `U(Meta)` and `VmType` always inhabits `U(Meta)` too
            // (the meta universe classifies both), but `Type` is only valid as a type in
            // meta context and `VmType` only in object context.
            let ty_phase = match prim {
                Prim::IntTy(IntType { phase: p, .. }) => p,
                Prim::U(Phase::Meta) => Phase::Meta, // "Type"
                Prim::U(Phase::Object) => Phase::Object, // "VmType"
                Prim::Add(_)
                | Prim::Sub(_)
                | Prim::Mul(_)
                | Prim::Div(_)
                | Prim::BitAnd(_)
                | Prim::BitOr(_)
                | Prim::BitNot(_)
                | Prim::Embed(_)
                | Prim::Eq(_)
                | Prim::Ne(_)
                | Prim::Lt(_)
                | Prim::Gt(_)
                | Prim::Le(_)
                | Prim::Ge(_) => unreachable!("builtin_prim only returns IntTy or U"),
            };
            if ty_phase != phase {
                return Err(anyhow!(
                    "`{}` is a {ty_phase}-phase type, not valid in a {phase}-phase type position",
                    name.as_str(),
                ));
            }
            Ok(arena.alloc(core::Term::Prim(prim)))
        }
        ast::Term::Lift(inner) => {
            // `[[T]]` is only a valid type in a meta-level function signature.
            if phase != Phase::Meta {
                return Err(anyhow!(
                    "`[[...]]` is only valid in a meta-phase type position"
                ));
            }
            // The inner type must be an object type.
            let inner_ty = elaborate_ty(arena, Phase::Object, inner)?;
            Ok(arena.alloc(core::Term::Lift(inner_ty)))
        }
        ast::Term::Lit(_)
        | ast::Term::App { .. }
        | ast::Term::Quote(_)
        | ast::Term::Splice(_)
        | ast::Term::Match { .. }
        | ast::Term::Block { .. } => Err(anyhow!("expected a type expression")),
    }
}

/// Elaborate the signature (parameter types + return type) of a single function.
fn elaborate_sig<'src, 'core>(
    arena: &'core bumpalo::Bump,
    func: &ast::Function<'src>,
) -> Result<core::FunSig<'core>> {
    let params: &'core [(&'core str, &'core core::Term<'core>)] =
        arena.alloc_slice_try_fill_iter(func.params.iter().map(|p| -> Result<_> {
            let param_name: &'core str = arena.alloc_str(p.name.as_str());
            let param_ty = elaborate_ty(arena, func.phase, p.ty)?;
            Ok((param_name, param_ty))
        }))?;

    let ret_ty = elaborate_ty(arena, func.phase, func.ret_ty)?;

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

        if globals.contains_key(&name) {
            return Err(anyhow!("duplicate function name `{name}`"));
        }

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
            let sig = globals.get(&name).expect("signature missing from pass 1");

            // Build a fresh context borrowing the stack-owned globals map.
            let mut ctx = Ctx::new(arena, globals);

            // Push parameters as locals so the body can reference them.
            for (pname, pty) in sig.params {
                ctx.push_local(pname, pty);
            }

            // Elaborate the body, checking it against the declared return type.
            let core_body = check(&mut ctx, sig.phase, func.body, sig.ret_ty)
                .with_context(|| format!("in function `{name}`"))?;

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
        | core::Term::Lit(_)
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
            let builtin = builtin_prim(name_str, phase);
            if let Some(prim) = builtin {
                let term = ctx.alloc(core::Term::Prim(prim));
                // The type of a type is the relevant universe.
                // U(Meta) : U(Meta)   — type-in-type for the meta universe
                // U(Object) : U(Meta) — VmType is classified by the meta universe
                // Both arms return U(Meta) for distinct semantic reasons; keep them separate.
                #[expect(clippy::match_same_arms)]
                let ty = match prim {
                    Prim::IntTy(_) => ctx.alloc(core::Term::Prim(Prim::U(phase))),
                    Prim::U(Phase::Meta) => ctx.alloc(core::Term::Prim(Prim::U(Phase::Meta))),
                    Prim::U(Phase::Object) => ctx.alloc(core::Term::Prim(Prim::U(Phase::Meta))),
                    Prim::Add(_)
                    | Prim::Sub(_)
                    | Prim::Mul(_)
                    | Prim::Div(_)
                    | Prim::BitAnd(_)
                    | Prim::BitOr(_)
                    | Prim::BitNot(_)
                    | Prim::Embed(_)
                    | Prim::Eq(_)
                    | Prim::Ne(_)
                    | Prim::Lt(_)
                    | Prim::Gt(_)
                    | Prim::Le(_)
                    | Prim::Ge(_) => unreachable!(),
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
            let sig = ctx
                .globals
                .get(name)
                .ok_or_else(|| anyhow!("unknown function `{name}`"))?;

            // The call phase must match the current elaboration phase.
            if sig.phase != phase {
                return Err(anyhow!(
                    "function `{name}` is a {}-phase function, but called in {}-phase context",
                    sig.phase,
                    phase,
                ));
            }
            let call_phase = sig.phase;
            let ret_ty = sig.ret_ty;
            let params = sig.params;

            if args.len() != params.len() {
                return Err(anyhow!(
                    "function `{name}` expects {} argument(s), got {}",
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

            let core_term = ctx.alloc(core::Term::new_app(
                core::Head::Global(core::Name::new(ctx.arena.alloc_str(name.as_str()))),
                core_args,
            ));
            Ok((core_term, ret_ty))
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
            if args.len() != 2 {
                return Err(anyhow!("binary operation expects exactly 2 arguments"));
            }
            // Infer the operand type from the first argument.
            #[expect(clippy::indexing_slicing)]
            let (core_arg0, operand_ty) = infer(ctx, phase, args[0])?;
            // Check the second argument against the same operand type.
            #[expect(clippy::indexing_slicing)]
            let core_arg1 = check(ctx, phase, args[1], operand_ty)?;
            // Verify both operands are integers and build the prim carrying the operand type.
            let op_int_ty = match operand_ty {
                core::Term::Prim(Prim::IntTy(it)) => *it,
                core::Term::Var(_)
                | core::Term::Prim(_)
                | core::Term::Lit(_)
                | core::Term::App(_)
                | core::Term::Lift(_)
                | core::Term::Quote(_)
                | core::Term::Splice(_)
                | core::Term::Let(_)
                | core::Term::Match(_) => {
                    return Err(anyhow!("comparison operands must be integers"));
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
            let core_term = ctx.alloc(core::Term::new_app(core::Head::Prim(prim), core_args));
            // Result type is always u1 at the current phase.
            let u1_ty = ctx.alloc(core::Term::Prim(Prim::IntTy(IntType::new(
                IntWidth::U1,
                phase,
            ))));
            Ok((core_term, u1_ty))
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
            if phase != Phase::Meta {
                return Err(anyhow!("`[[...]]` is only valid in a meta-phase context"));
            }
            // The inner expression must be an object type.
            let (core_inner, inner_ty) = infer(ctx, Phase::Object, inner)?;
            // Verify the inner term is indeed a type (inhabits VmType).
            if !types_equal(inner_ty, &core::Term::Prim(Prim::U(Phase::Object))) {
                return Err(anyhow!("argument of `[[...]]` must be an object type"));
            }
            let lift_term = ctx.alloc(core::Term::Lift(core_inner));
            let ty = ctx.alloc(core::Term::Prim(Prim::U(Phase::Meta)));
            Ok((lift_term, ty))
        }

        // ------------------------------------------------------------------ Quote
        // `#(t)` — infer iff the inner term is inferable (phase shifts meta→object).
        ast::Term::Quote(inner) => {
            // Quote is only legal in meta phase.
            if phase != Phase::Meta {
                return Err(anyhow!("`#(...)` is only valid in a meta-phase context"));
            }
            let (core_inner, inner_ty) = infer(ctx, Phase::Object, inner)?;
            let lift_ty = ctx.alloc(core::Term::Lift(inner_ty));
            let core_term = ctx.alloc(core::Term::Quote(core_inner));
            Ok((core_term, lift_ty))
        }

        // ------------------------------------------------------------------ Splice
        // `$(t)` — infer iff `t` infers as `[[T]]`; result type is `T` (phase shifts object→meta).
        // If `t` infers as a meta integer `IntTy(w, Meta)`, insert an implicit `Embed(w)`
        // to produce `[[IntTy(w, Object)]]` before splicing.
        ast::Term::Splice(inner) => {
            // Splice is only legal in object phase.
            if phase != Phase::Object {
                return Err(anyhow!("`$(...)` is only valid in an object-phase context"));
            }
            let (core_inner, inner_ty) = infer(ctx, Phase::Meta, inner)?;
            match inner_ty {
                core::Term::Lift(object_ty) => {
                    let core_term = ctx.alloc(core::Term::Splice(core_inner));
                    Ok((core_term, object_ty))
                }
                // A meta-level integer is implicitly embedded: insert Embed(w) so that
                // the splice argument has type `[[IntTy(w, Object)]]`.
                core::Term::Prim(Prim::IntTy(IntType {
                    width,
                    phase: Phase::Meta,
                })) => {
                    let obj_ty = ctx.alloc(core::Term::Prim(Prim::IntTy(IntType::new(
                        *width,
                        Phase::Object,
                    ))));
                    let embedded = ctx.alloc(core::Term::new_app(
                        core::Head::Prim(Prim::Embed(*width)),
                        ctx.alloc_slice([core_inner]),
                    ));
                    let core_term = ctx.alloc(core::Term::Splice(embedded));
                    Ok((core_term, obj_ty))
                }
                core::Term::Var(_)
                | core::Term::Prim(_)
                | core::Term::Lit(_)
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
        core::Term::Prim(Prim::IntTy(IntType {
            width: IntWidth::U0,
            ..
        })) => Some(vec![false; 1]),
        core::Term::Prim(Prim::IntTy(IntType {
            width: IntWidth::U1,
            ..
        })) => Some(vec![false; 2]),
        core::Term::Prim(Prim::IntTy(IntType {
            width: IntWidth::U8,
            ..
        })) => Some(vec![false; 256]),
        core::Term::Var(_)
        | core::Term::Prim(_)
        | core::Term::Lit(_)
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
                    // Out-of-range literal for the type — the pattern can never
                    // match, but we don't hard-error here; the body is still
                    // elaborated and will catch type errors if any.
                    let Ok(idx) = usize::try_from(*n) else {
                        continue;
                    };
                    if let Some(bit) = bits.get_mut(idx) {
                        *bit = true;
                    }
                }
            }
        }
    }

    let fully_covered = covered_lits.is_some_and(|bits| bits.iter().all(|&b| b));
    if !has_catch_all && !fully_covered {
        return Err(anyhow!(
            "match expression is not exhaustive: no wildcard or bind-all arm"
        ));
    }
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
        let ty = elaborate_ty(ctx.arena, phase, ann)?;
        let core_e = check(ctx, phase, stmt.expr, ty)
            .with_context(|| format!("in let binding `{}`", stmt.name.as_str()))?;
        (core_e, ty)
    } else {
        infer(ctx, phase, stmt.expr)
            .with_context(|| format!("in let binding `{}`", stmt.name.as_str()))?
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
) -> Result<(&'core core::Term<'core>, &'core core::Term<'core>)> {
    match stmts {
        [] => infer(ctx, phase, expr),
        [first, rest @ ..] => elaborate_let(
            ctx,
            phase,
            first,
            |ctx| infer_block(ctx, phase, rest, expr),
            |(body, _ty)| body,
            |let_term, (_body, ty)| (let_term, ty),
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
    if ty_phase != phase {
        return Err(anyhow!(
            "expected type inhabits the {ty_phase}-phase universe, \
             but elaborating at {phase} phase"
        ));
    }
    match term {
        // ------------------------------------------------------------------ Lit
        // Literals check against any integer type.
        ast::Term::Lit(n) => match expected {
            core::Term::Prim(Prim::IntTy(_)) => Ok(ctx.alloc(core::Term::Lit(*n))),
            core::Term::Var(_)
            | core::Term::Prim(_)
            | core::Term::Lit(_)
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
                | core::Term::Lit(_)
                | core::Term::App { .. }
                | core::Term::Lift(_)
                | core::Term::Quote(_)
                | core::Term::Splice(_)
                | core::Term::Let { .. }
                | core::Term::Match { .. } => {
                    return Err(anyhow!("primitive operation requires an integer type"));
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

            if args.len() != 2 {
                return Err(anyhow!("binary operation expects exactly 2 arguments"));
            }

            #[expect(clippy::indexing_slicing)]
            let core_arg0 = check(ctx, phase, args[0], expected)?;
            #[expect(clippy::indexing_slicing)]
            let core_arg1 = check(ctx, phase, args[1], expected)?;

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
                | core::Term::Lit(_)
                | core::Term::App(_)
                | core::Term::Lift(_)
                | core::Term::Quote(_)
                | core::Term::Splice(_)
                | core::Term::Let(_)
                | core::Term::Match(_) => {
                    return Err(anyhow!("primitive operation requires an integer type"));
                }
            };

            let prim = match op {
                ast::UnOp::Not => Prim::BitNot(int_ty),
            };

            if args.len() != 1 {
                return Err(anyhow!("unary operation expects exactly 1 argument"));
            }
            #[expect(clippy::indexing_slicing)]
            let core_arg = check(ctx, phase, args[0], expected)?;
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
            | core::Term::Lit(_)
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
            if phase != Phase::Object {
                return Err(anyhow!("`$(...)` is only valid in an object-phase context"));
            }
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
                let meta_int_ty = ctx.alloc(core::Term::Prim(Prim::IntTy(IntType::new(
                    *width,
                    Phase::Meta,
                ))));
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
            let (core_scrutinee, scrut_ty) = infer(ctx, phase, scrutinee)?;

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
