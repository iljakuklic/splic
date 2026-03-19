//! Checker behaviour tests - Match

use super::*;

// `match x { 0 => k32(), _ => k32() }` checks against u32.
#[test]
fn check_match_all_arms_same_type_succeeds() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();

    let u32_ty_core = core_arena.alloc(core::Term::Prim(Prim::IntTy(IntType::new(
        IntWidth::U32,
        Phase::Meta,
    ))));
    let mut globals = HashMap::new();
    globals.insert(
        Name::new("k32"),
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

    assert!(check(&mut ctx, Phase::Meta, term, u32_ty_core).is_ok());
}

// A match on u1 covering both 0 and 1 with no wildcard is exhaustive — must succeed.
#[test]
fn check_match_u1_fully_covered_succeeds() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();

    let u1_ty_core = core_arena.alloc(core::Term::Prim(Prim::IntTy(IntType::new(
        IntWidth::U1,
        Phase::Meta,
    ))));
    let mut globals = HashMap::new();
    globals.insert(
        Name::new("k1"),
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

    assert!(check(&mut ctx, Phase::Meta, term, u1_ty_core).is_ok());
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
        Name::new("k1"),
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
        Name::new("k32"),
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
        Name::new("k32"),
        FunSig {
            params: &[],
            ret_ty: u32_ty_core,
            phase: Phase::Meta,
        },
    );
    globals.insert(
        Name::new("k64"),
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
