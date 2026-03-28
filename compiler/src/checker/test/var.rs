//! Checker behaviour tests — Var

use super::*;

// `infer` on a `Var` looks it up in locals and returns its type.
#[test]
fn infer_var_in_scope_returns_its_type() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);
    let u32_ty = &core::Term::U32_META;
    ctx.push_local(core::Name::new("x"), u32_ty);

    let term = src_arena.alloc(ast::Term::Var(ast::Name::new("x")));
    let (core_term, ty_val) = infer(&mut ctx, Phase::Meta, term).expect("should infer");
    // With one local "x", infer returns Var(Ix(0)) — innermost (only) binder.
    assert!(matches!(core_term, core::Term::Var(Ix(0))));
    assert!(matches!(
        ty_val,
        value::Value::Prim(Prim::IntTy(IntType {
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

// With two locals the correct De Bruijn index is returned.
#[test]
fn infer_var_returns_correct_index() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);
    let u64_ty = &core::Term::U64_META;
    let u32_ty = &core::Term::U32_META;
    ctx.push_local(core::Name::new("x"), u64_ty); // outer: index 1
    ctx.push_local(core::Name::new("y"), u32_ty); // inner: index 0

    let term = src_arena.alloc(ast::Term::Var(ast::Name::new("y")));
    let (core_term, _) = infer(&mut ctx, Phase::Meta, term).expect("should infer");
    // "y" is innermost, so Ix(0).
    assert!(matches!(core_term, core::Term::Var(Ix(0))));
}

// Shadowing: the innermost binding wins.
#[test]
fn infer_var_shadowed_returns_innermost() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);
    let u64_ty = &core::Term::U64_META;
    let u32_ty = &core::Term::U32_META;
    ctx.push_local(core::Name::new("x"), u64_ty); // outer x: u64, index 1
    ctx.push_local(core::Name::new("x"), u32_ty); // inner x: u32 — shadows, index 0

    let term = src_arena.alloc(ast::Term::Var(ast::Name::new("x")));
    let (core_term, ty_val) = infer(&mut ctx, Phase::Meta, term).expect("should infer");
    // Innermost "x" is at Ix(0).
    assert!(matches!(core_term, core::Term::Var(Ix(0))));
    assert!(matches!(
        ty_val,
        value::Value::Prim(Prim::IntTy(IntType {
            width: IntWidth::U32,
            ..
        }))
    ));
}
