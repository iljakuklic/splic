//! Checker behaviour tests — Var

use super::*;

// `infer` on a `Var` looks it up in locals and returns its type.
#[test]
fn infer_var_in_scope_returns_its_type() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);
    let u32_ty = ctx.u32_ty();
    ctx.push_local("x", u32_ty);

    let term = src_arena.alloc(ast::Term::Var(ast::Name::new("x")));
    let (core_term, ty) = infer(&mut ctx, Phase::Meta, term).expect("should infer");
    assert!(matches!(core_term, core::Term::Var(Lvl(0))));
    assert!(matches!(
        ty,
        core::Term::Prim(Prim::IntTy(IntType {
            width: IntWidth::U32,
            ..
        }))
    ));
}

// Inferring an unbound variable must fail.
#[test]
fn infer_var_out_of_scope_fails() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);

    let term = src_arena.alloc(ast::Term::Var(ast::Name::new("x")));
    assert!(infer(&mut ctx, Phase::Meta, term).is_err());
}

// With two locals the correct De Bruijn level is returned.
#[test]
fn infer_var_returns_correct_level() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);
    let u64_ty = ctx.u64_ty();
    let u32_ty = ctx.u32_ty();
    ctx.push_local("x", u64_ty); // level 0
    ctx.push_local("y", u32_ty); // level 1

    let term = src_arena.alloc(ast::Term::Var(ast::Name::new("y")));
    let (core_term, _) = infer(&mut ctx, Phase::Meta, term).expect("should infer");
    assert!(matches!(core_term, core::Term::Var(Lvl(1))));
}

// Shadowing: the innermost binding wins.
#[test]
fn infer_var_shadowed_returns_innermost() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);
    let u64_ty = ctx.u64_ty();
    let u32_ty = ctx.u32_ty();
    ctx.push_local("x", u64_ty); // level 0, u64
    ctx.push_local("x", u32_ty); // level 1, u32 — shadows

    let term = src_arena.alloc(ast::Term::Var(ast::Name::new("x")));
    let (core_term, ty) = infer(&mut ctx, Phase::Meta, term).expect("should infer");
    assert!(matches!(core_term, core::Term::Var(Lvl(1))));
    assert!(matches!(
        ty,
        core::Term::Prim(Prim::IntTy(IntType {
            width: IntWidth::U32,
            ..
        }))
    ));
}
