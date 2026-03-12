//! Checker behaviour tests - Match

use super::*;

// `match x { 0 => 10u32, _ => 20u32 }` infers as u32.
#[test]
fn infer_match_all_arms_same_type_succeeds() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);
    let u32_ty = ctx.u32_ty();
    ctx.push_local("x", u32_ty);

    // scrutinee: x
    let scrutinee = src_arena.alloc(ast::Term::Var(ast::Name::new("x")));

    // arm 0 => some_global_returning_u32
    // We use globals to give the arm bodies inferable types.
    // Simpler: use annotated lets inside. Even simpler for this test:
    // use a global call on each arm.
    //
    // Actually the simplest approach: both arms are `Var`s bound by the
    // match arm pattern (binding pattern). We set up two checked literals
    // via a helper global instead.
    //
    // For now use a single wildcard arm whose body is a global call.
    let mut globals = HashMap::new();
    globals.insert("k", sig_no_params_returns_u64(&core_arena));
    // k() -> u64; but we want u32. Use a zero-arg fn returning u32 instead.
    let u32_ty_core = core_arena.alloc(core::Term::Prim(Prim::IntTy(IntType::new(
        IntWidth::U32,
        Phase::Meta,
    ))));
    globals.insert(
        "k32",
        FunSig {
            params: &[],
            ret_ty: u32_ty_core,
            phase: Phase::Meta,
        },
    );
    let mut ctx = test_ctx_with_globals(&core_arena, &globals);
    let u32_ty = ctx.u32_ty();
    ctx.push_local("x", u32_ty);

    let arm0_body = src_arena.alloc(ast::Term::App {
        func: FunName::Name(ast::Name::new("k32")),
        args: &[],
    });
    let arm1_body = src_arena.alloc(ast::Term::App {
        func: FunName::Name(ast::Name::new("k32")),
        args: &[],
    });
    let arms = src_arena.alloc_slice_fill_iter([
        MatchArm {
            pat: ast::Pat::Lit(0),
            body: arm0_body,
        },
        MatchArm {
            pat: ast::Pat::Name(ast::Name::new("_")),
            body: arm1_body,
        },
    ]);
    let term = src_arena.alloc(ast::Term::Match { scrutinee, arms });

    let (_, ty) = infer(&mut ctx, Phase::Meta, term).expect("should infer");
    assert!(matches!(
        ty,
        core::Term::Prim(Prim::IntTy(IntType {
            width: IntWidth::U32,
            ..
        }))
    ));
}

// A match on u1 covering both 0 and 1 with no wildcard is exhaustive — must succeed.
#[test]
fn infer_match_u1_fully_covered_succeeds() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();

    let u1_ty_core = core_arena.alloc(core::Term::Prim(Prim::IntTy(IntType::new(
        IntWidth::U1,
        Phase::Meta,
    ))));
    let mut globals = HashMap::new();
    globals.insert(
        "k1",
        FunSig {
            params: &[],
            ret_ty: u1_ty_core,
            phase: Phase::Meta,
        },
    );
    let mut ctx = test_ctx_with_globals(&core_arena, &globals);
    ctx.push_local("x", u1_ty_core);

    let scrutinee = src_arena.alloc(ast::Term::Var(ast::Name::new("x")));
    let arm0_body = src_arena.alloc(ast::Term::App {
        func: FunName::Name(ast::Name::new("k1")),
        args: &[],
    });
    let arm1_body = src_arena.alloc(ast::Term::App {
        func: FunName::Name(ast::Name::new("k1")),
        args: &[],
    });
    // Both values of u1 are covered — exhaustive without a wildcard.
    let arms = src_arena.alloc_slice_fill_iter([
        ast::MatchArm {
            pat: ast::Pat::Lit(0),
            body: arm0_body,
        },
        ast::MatchArm {
            pat: ast::Pat::Lit(1),
            body: arm1_body,
        },
    ]);
    let term = src_arena.alloc(ast::Term::Match { scrutinee, arms });

    assert!(infer(&mut ctx, Phase::Meta, term).is_ok());
}

// A match on u1 covering only 0 with no wildcard is not exhaustive — must fail.
#[test]
fn infer_match_u1_partially_covered_fails() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();

    let u1_ty_core = core_arena.alloc(core::Term::Prim(Prim::IntTy(IntType::new(
        IntWidth::U1,
        Phase::Meta,
    ))));
    let mut globals = HashMap::new();
    globals.insert(
        "k1",
        FunSig {
            params: &[],
            ret_ty: u1_ty_core,
            phase: Phase::Meta,
        },
    );
    let mut ctx = test_ctx_with_globals(&core_arena, &globals);
    ctx.push_local("x", u1_ty_core);

    let scrutinee = src_arena.alloc(ast::Term::Var(ast::Name::new("x")));
    let arm0_body = src_arena.alloc(ast::Term::App {
        func: FunName::Name(ast::Name::new("k1")),
        args: &[],
    });
    // Only 0 covered, 1 is missing — not exhaustive.
    let arms = src_arena.alloc_slice_fill_iter([ast::MatchArm {
        pat: ast::Pat::Lit(0),
        body: arm0_body,
    }]);
    let term = src_arena.alloc(ast::Term::Match { scrutinee, arms });

    assert!(infer(&mut ctx, Phase::Meta, term).is_err());
}

// A match with only literal arms and no catch-all is not exhaustive — must fail.
#[test]
fn infer_match_no_catch_all_fails() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();

    let u32_ty_core = core_arena.alloc(core::Term::Prim(Prim::IntTy(IntType::new(
        IntWidth::U32,
        Phase::Meta,
    ))));
    let mut globals = HashMap::new();
    globals.insert(
        "k32",
        FunSig {
            params: &[],
            ret_ty: u32_ty_core,
            phase: Phase::Meta,
        },
    );
    let mut ctx = test_ctx_with_globals(&core_arena, &globals);
    let u32_ty = ctx.u32_ty();
    ctx.push_local("x", u32_ty);

    let scrutinee = src_arena.alloc(ast::Term::Var(ast::Name::new("x")));
    let arm0_body = src_arena.alloc(ast::Term::App {
        func: FunName::Name(ast::Name::new("k32")),
        args: &[],
    });
    let arm1_body = src_arena.alloc(ast::Term::App {
        func: FunName::Name(ast::Name::new("k32")),
        args: &[],
    });
    // Only literal arms, no wildcard/bind — not exhaustive.
    let arms = src_arena.alloc_slice_fill_iter([
        ast::MatchArm {
            pat: ast::Pat::Lit(0),
            body: arm0_body,
        },
        ast::MatchArm {
            pat: ast::Pat::Lit(1),
            body: arm1_body,
        },
    ]);
    let term = src_arena.alloc(ast::Term::Match { scrutinee, arms });

    assert!(infer(&mut ctx, Phase::Meta, term).is_err());
}

// Arms that return different types must fail.
#[test]
fn infer_match_arms_type_mismatch_fails() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();

    let u32_ty_core = core_arena.alloc(core::Term::Prim(Prim::IntTy(IntType::new(
        IntWidth::U32,
        Phase::Meta,
    ))));
    let u64_ty_core = core_arena.alloc(core::Term::Prim(Prim::IntTy(IntType::new(
        IntWidth::U64,
        Phase::Meta,
    ))));
    let mut globals = HashMap::new();
    globals.insert(
        "k32",
        FunSig {
            params: &[],
            ret_ty: u32_ty_core,
            phase: Phase::Meta,
        },
    );
    globals.insert(
        "k64",
        FunSig {
            params: &[],
            ret_ty: u64_ty_core,
            phase: Phase::Meta,
        },
    );
    let mut ctx = test_ctx_with_globals(&core_arena, &globals);
    let u32_ty = ctx.u32_ty();
    ctx.push_local("x", u32_ty);

    let scrutinee = src_arena.alloc(ast::Term::Var(ast::Name::new("x")));
    let arm0_body = src_arena.alloc(ast::Term::App {
        func: FunName::Name(ast::Name::new("k32")),
        args: &[],
    });
    let arm1_body = src_arena.alloc(ast::Term::App {
        func: FunName::Name(ast::Name::new("k64")),
        args: &[],
    });
    let arms = src_arena.alloc_slice_fill_iter([
        MatchArm {
            pat: ast::Pat::Lit(0),
            body: arm0_body,
        },
        MatchArm {
            pat: ast::Pat::Name(ast::Name::new("_")),
            body: arm1_body,
        },
    ]);
    let term = src_arena.alloc(ast::Term::Match { scrutinee, arms });

    assert!(infer(&mut ctx, Phase::Meta, term).is_err());
}
