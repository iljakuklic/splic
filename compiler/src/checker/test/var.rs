//! Checker behaviour tests — Var

use super::*;

// `infer` on a `Var` looks it up in locals and returns its type.
#[test]
fn infer_var_in_scope_returns_its_type() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);
    let u32_ty = &core::Term::U32_META;
    ctx.push_local(core::Name::new("x"), u32_ty, Phase::Meta);

    let term = src_arena.alloc(ast::Term::Var(ast::Name::new("x")));
    let (core_term, ty_val) = infer(&mut ctx, Phase::Meta, term).expect("should infer");
    // With one local "x", infer returns Var(de_bruijn::Ix(0)) — innermost (only) binder.
    assert_eq!(core_term, &core::Term::Var(de_bruijn::Ix::new(0)));
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
    ctx.push_local(core::Name::new("x"), u64_ty, Phase::Meta); // outer: index 1
    ctx.push_local(core::Name::new("y"), u32_ty, Phase::Meta); // inner: index 0

    let term = src_arena.alloc(ast::Term::Var(ast::Name::new("y")));
    let (core_term, _) = infer(&mut ctx, Phase::Meta, term).expect("should infer");
    // "y" is innermost, so de_bruijn::Ix(0).
    assert_eq!(core_term, &core::Term::Var(de_bruijn::Ix::new(0)));
}

// Shadowing: the innermost binding wins.
#[test]
fn infer_var_shadowed_returns_innermost() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);
    let u64_ty = &core::Term::U64_META;
    let u32_ty = &core::Term::U32_META;
    ctx.push_local(core::Name::new("x"), u64_ty, Phase::Meta); // outer x: u64, index 1
    ctx.push_local(core::Name::new("x"), u32_ty, Phase::Meta); // inner x: u32 — shadows, index 0

    let term = src_arena.alloc(ast::Term::Var(ast::Name::new("x")));
    let (core_term, ty_val) = infer(&mut ctx, Phase::Meta, term).expect("should infer");
    // Innermost "x" is at de_bruijn::Ix(0).
    assert_eq!(core_term, &core::Term::Var(de_bruijn::Ix::new(0)));
    assert!(matches!(
        ty_val,
        value::Value::Prim(Prim::IntTy(IntType {
            width: IntWidth::U32,
            ..
        }))
    ));
}

// An object-phase variable may be referenced in object-phase code.
#[test]
fn infer_object_var_in_object_phase_succeeds() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);
    ctx.push_local(core::Name::new("x"), &core::Term::U64_OBJ, Phase::Object);

    let term = src_arena.alloc(ast::Term::Var(ast::Name::new("x")));
    let (core_term, _) = infer(&mut ctx, Phase::Object, term).expect("should infer");
    assert_eq!(core_term, &core::Term::Var(de_bruijn::Ix::new(0)));
}

// An object-phase variable referenced in meta-phase code is a phase error,
// not a deferred staging failure (issue #27).
#[test]
fn infer_object_var_in_meta_phase_fails() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);
    ctx.push_local(core::Name::new("x"), &core::Term::U64_OBJ, Phase::Object);

    let term = src_arena.alloc(ast::Term::Var(ast::Name::new("x")));
    let err = infer(&mut ctx, Phase::Meta, term).expect_err("should reject");
    assert!(
        err.to_string().contains("object-phase variable `x`"),
        "unexpected error: {err}"
    );
}

// A meta-phase variable referenced in object-phase code is a phase error,
// even when its type happens to be an object type's lift (issue #27).
#[test]
fn infer_meta_var_in_object_phase_fails() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);
    let lifted = core_arena.alloc(core::Term::Lift(&core::Term::U64_OBJ));
    ctx.push_local(core::Name::new("x"), lifted, Phase::Meta);

    let term = src_arena.alloc(ast::Term::Var(ast::Name::new("x")));
    let err = infer(&mut ctx, Phase::Object, term).expect_err("should reject");
    assert!(
        err.to_string().contains("meta-phase variable `x`"),
        "unexpected error: {err}"
    );
}

// Shadowing respects phase: the innermost binding's phase is the one checked.
#[test]
fn infer_var_shadowed_uses_innermost_phase() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);
    ctx.push_local(core::Name::new("x"), &core::Term::U64_META, Phase::Meta);
    ctx.push_local(core::Name::new("x"), &core::Term::U64_OBJ, Phase::Object);

    let term = src_arena.alloc(ast::Term::Var(ast::Name::new("x")));
    assert!(infer(&mut ctx, Phase::Object, term).is_ok());
    assert!(infer(&mut ctx, Phase::Meta, term).is_err());
}

// A meta global referenced from object-phase code is a phase error (issue #27).
#[test]
fn infer_meta_global_in_object_phase_fails() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut globals = HashMap::new();
    globals.insert(Name::new("f"), sig_no_params_returns_u64(&core_arena));
    let mut ctx = test_ctx_with_globals(&core_arena, &globals);

    let term = src_arena.alloc(ast::Term::Var(ast::Name::new("f")));
    assert!(infer(&mut ctx, Phase::Meta, term).is_ok());
    let err = infer(&mut ctx, Phase::Object, term).expect_err("should reject");
    assert!(
        err.to_string().contains("meta-phase definition `f`"),
        "unexpected error: {err}"
    );
}
