//! Checker behaviour tests — Lit

use super::*;

// `check` a literal against its declared type succeeds and produces `Lit`.
#[test]
fn check_lit_against_matching_int_type_succeeds() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);
    let expected = core::Term::int_ty(IntWidth::U32, Phase::Object);

    let term = src_arena.alloc(ast::Term::Lit(42));
    let result = check(&mut ctx, Phase::Object, term, expected).expect("should check");
    assert!(matches!(result, core::Term::Lit(42, _)));
}

// `check` at meta phase with an object-phase expected type must fail (universe mismatch).
#[test]
fn check_meta_term_against_object_type_fails() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);
    let obj_u32 = core::Term::int_ty(IntWidth::U32, Phase::Object);
    let term = src_arena.alloc(ast::Term::Lit(42));
    assert!(check(&mut ctx, Phase::Meta, term, obj_u32).is_err());
}

// `check` at object phase with a meta-phase expected type must fail (universe mismatch).
#[test]
fn check_object_term_against_meta_type_fails() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);
    let meta_u32 = &core::Term::U32_META; // u32 at meta phase
    let term = src_arena.alloc(ast::Term::Lit(42));
    assert!(check(&mut ctx, Phase::Object, term, meta_u32).is_err());
}

// `check` a literal against a non-integer type (universe) must fail.
#[test]
fn check_lit_against_universe_fails() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);
    let expected = &core::Term::TYPE; // Type, not an integer type

    let term = src_arena.alloc(ast::Term::Lit(42));
    assert!(check(&mut ctx, Phase::Meta, term, expected).is_err());
}

// `infer` on a bare literal (no annotation) must fail — literals are check-only.
#[test]
fn infer_lit_fails() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);

    let term = src_arena.alloc(ast::Term::Lit(0));
    assert!(infer(&mut ctx, Phase::Meta, term).is_err());
}

// Test that literals exceeding the maximum value for their type are rejected.
#[rstest::rstest]
#[case(IntWidth::U0, 1)] // u0 max is 0
#[case(IntWidth::U1, 2)] // u1 max is 1
#[case(IntWidth::U8, u64::from(u8::MAX) + 1)]
#[case(IntWidth::U16, u64::from(u16::MAX) + 1)]
#[case(IntWidth::U32, u64::from(u32::MAX) + 1)]
fn check_lit_exceeds_max_fails(#[case] width: IntWidth, #[case] value: u64) {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);
    let expected = core::Term::int_ty(width, Phase::Object);

    let term = src_arena.alloc(ast::Term::Lit(value));
    assert!(check(&mut ctx, Phase::Object, term, expected).is_err());
}

// Test that literals at the maximum value for their type succeed.
#[rstest::rstest]
#[case(IntWidth::U0, 0)]
#[case(IntWidth::U1, 1)]
#[case(IntWidth::U8, u64::from(u8::MAX))]
#[case(IntWidth::U16, u64::from(u16::MAX))]
#[case(IntWidth::U32, u64::from(u32::MAX))]
#[case(IntWidth::U64, u64::MAX)]
fn check_lit_at_max_succeeds(#[case] width: IntWidth, #[case] value: u64) {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);
    let expected = core::Term::int_ty(width, Phase::Object);

    let term = src_arena.alloc(ast::Term::Lit(value));
    let result = check(&mut ctx, Phase::Object, term, expected).expect("should check");
    assert!(matches!(result, core::Term::Lit(v, _) if *v == value));
}
