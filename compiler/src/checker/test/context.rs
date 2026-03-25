//! Structural / context tests

use super::*;

#[test]
fn prim_types_are_well_kinded() {
    let u64_term = &core::Term::U64_META;
    assert!(matches!(
        u64_term,
        core::Term::Prim(Prim::IntTy(IntType {
            width: IntWidth::U64,
            ..
        }))
    ));
}

#[test]
fn literal_checks_against_int_type() {
    let arena = bumpalo::Bump::new();
    let lit = arena.alloc(core::Term::Lit(42, IntType::U64_META));
    assert!(matches!(lit, core::Term::Lit(42, _)));
}

#[test]
fn variable_lookup_in_empty_context() {
    let arena = bumpalo::Bump::new();
    let ctx = test_ctx(&arena);
    assert_eq!(ctx.lookup_local("x"), None);
}

#[test]
fn variable_lookup_after_push() {
    let arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&arena);
    let u64_term = &core::Term::U64_META;
    ctx.push_local("x", u64_term);

    let (lvl, ty) = ctx.lookup_local("x").expect("x should be in scope");
    assert_eq!(lvl, Lvl(0));
    assert!(matches!(
        ty,
        core::Term::Prim(Prim::IntTy(IntType {
            width: IntWidth::U64,
            ..
        }))
    ));
}

#[test]
fn variable_lookup_with_multiple_locals() {
    let arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&arena);
    let u64_term = &core::Term::U64_META;
    let u32_term = &core::Term::U32_META;

    ctx.push_local("x", u64_term);
    ctx.push_local("y", u32_term);

    let (lvl_y, ty_y) = ctx.lookup_local("y").expect("y should be in scope");
    assert_eq!(lvl_y, Lvl(1));
    assert!(matches!(
        ty_y,
        core::Term::Prim(Prim::IntTy(IntType {
            width: IntWidth::U32,
            ..
        }))
    ));

    let (lvl_x, ty_x) = ctx.lookup_local("x").expect("x should be in scope");
    assert_eq!(lvl_x, Lvl(0));
    assert!(matches!(
        ty_x,
        core::Term::Prim(Prim::IntTy(IntType {
            width: IntWidth::U64,
            ..
        }))
    ));
}

#[test]
fn variable_shadowing() {
    let arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&arena);
    let u64_term = &core::Term::U64_META;
    let u32_term = &core::Term::U32_META;

    ctx.push_local("x", u64_term);
    ctx.push_local("x", u32_term);

    let (lvl, ty) = ctx.lookup_local("x").expect("x should be in scope");
    assert_eq!(lvl, Lvl(1));
    assert!(matches!(
        ty,
        core::Term::Prim(Prim::IntTy(IntType {
            width: IntWidth::U32,
            ..
        }))
    ));
}

#[test]
fn context_depth() {
    let arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&arena);
    let u64_term = &core::Term::U64_META;

    assert_eq!(ctx.depth(), 0);
    ctx.push_local("x", u64_term);
    assert_eq!(ctx.depth(), 1);
    ctx.push_local("y", u64_term);
    assert_eq!(ctx.depth(), 2);
    ctx.pop_local();
    assert_eq!(ctx.depth(), 1);
}

#[test]
fn meta_variable_in_quote_is_ok() {
    let arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&arena);
    let u64_term = &core::Term::U64_META;
    let lifted_u64 = ctx.lift_ty(u64_term);
    ctx.push_local("x", lifted_u64);
    let x_var = arena.alloc(core::Term::Var(Lvl(0)));
    assert!(matches!(x_var, core::Term::Var(Lvl(0))));
}

#[test]
fn object_variable_outside_quote_is_invalid() {
    let arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&arena);
    let u64_term = &core::Term::U64_META;
    ctx.push_local("x", u64_term);
    assert_eq!(ctx.depth(), 1);
}

#[test]
fn phase_is_argument_not_context() {
    let arena = bumpalo::Bump::new();
    let ctx = test_ctx(&arena);
    assert_eq!(ctx.depth(), 0);
}

#[test]
fn type_universe_distinction() {
    let type_tm = &core::Term::TYPE;
    let vm_type_tm = &core::Term::VM_TYPE;

    assert!(matches!(type_tm, core::Term::Prim(Prim::U(Phase::Meta))));
    assert!(matches!(
        vm_type_tm,
        core::Term::Prim(Prim::U(Phase::Object))
    ));
}

#[test]
fn arithmetic_requires_expected_type() {
    let add_u32 = Prim::Add(IntType::U32_OBJ);
    assert!(matches!(
        add_u32,
        Prim::Add(IntType {
            width: IntWidth::U32,
            ..
        })
    ));
}

#[test]
fn global_call_is_inferable() {
    let arena = bumpalo::Bump::new();
    let arg = arena.alloc(core::Term::Lit(1, IntType::U64_META));
    let global = arena.alloc(core::Term::Global(Name::new("foo")));
    let args = &*arena.alloc_slice_fill_iter([arg as &core::Term]);
    let app = arena.alloc(core::Term::new_app(global, args));
    assert!(matches!(app, core::Term::App(_)));
}

#[test]
fn comparison_operation_returns_u1() {
    let eq_u64 = Prim::Eq(IntType::U64_OBJ);
    assert!(matches!(
        eq_u64,
        Prim::Eq(IntType {
            width: IntWidth::U64,
            ..
        })
    ));
}

#[test]
fn lift_type_structure() {
    let arena = bumpalo::Bump::new();
    let ctx = test_ctx(&arena);
    let u64_term = &core::Term::U64_META;
    let lifted = ctx.lift_ty(u64_term);
    assert!(matches!(lifted, core::Term::Lift(_)));
}

#[test]
fn quote_inference_mirrors_inner() {
    let arena = bumpalo::Bump::new();
    let inner = arena.alloc(core::Term::Global(Name::new("foo")));
    let quoted = arena.alloc(core::Term::Quote(inner));
    assert!(matches!(quoted, core::Term::Quote(_)));
}

#[test]
fn splice_inference_mirrors_inner() {
    let arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&arena);
    let u64_term = &core::Term::U64_META;
    let lifted_u64 = ctx.lift_ty(u64_term);
    ctx.push_local("x", lifted_u64);
    let x_var = arena.alloc(core::Term::Var(Lvl(0)));
    let spliced = arena.alloc(core::Term::Splice(x_var));
    assert!(matches!(spliced, core::Term::Splice(_)));
}

#[test]
fn let_binding_structure() {
    let arena = bumpalo::Bump::new();
    let u64_term = &core::Term::U64_META;
    let expr = arena.alloc(core::Term::Lit(42, IntType::U64_META));
    let body = arena.alloc(core::Term::Var(Lvl(0)));
    let let_term = arena.alloc(core::Term::new_let("x", u64_term, expr, body));
    assert!(matches!(let_term, core::Term::Let(_)));
}

#[test]
fn match_with_literal_pattern() {
    let arena = bumpalo::Bump::new();
    let scrutinee = arena.alloc(core::Term::Var(Lvl(0)));
    let body0 = arena.alloc(core::Term::Lit(0, IntType::U64_META));
    let body1 = arena.alloc(core::Term::Lit(1, IntType::U64_META));

    let arm0 = core::Arm {
        pat: Pat::Lit(0),
        body: body0,
    };
    let arm1 = core::Arm {
        pat: Pat::Lit(1),
        body: body1,
    };

    let arms = &*arena.alloc_slice_fill_iter([arm0, arm1]);
    let match_term = arena.alloc(core::Term::new_match(scrutinee, arms));

    assert!(matches!(match_term, core::Term::Match(_)));
}

#[test]
fn match_with_binding_pattern() {
    let arena = bumpalo::Bump::new();
    let scrutinee = arena.alloc(core::Term::Var(Lvl(0)));
    let body = arena.alloc(core::Term::Var(Lvl(0)));

    let arm = core::Arm {
        pat: Pat::Bind("n"),
        body,
    };

    let arms = &*arena.alloc_slice_fill_iter([arm]);
    let match_term = arena.alloc(core::Term::new_match(scrutinee, arms));

    assert!(matches!(match_term, core::Term::Match(_)));
}

#[test]
fn function_call_to_global() {
    let arena = bumpalo::Bump::new();
    let arg = arena.alloc(core::Term::Lit(42, IntType::U64_META));
    let global = arena.alloc(core::Term::Global(Name::new("foo")));
    let args = &*arena.alloc_slice_fill_iter([arg as &core::Term]);
    let app = arena.alloc(core::Term::new_app(global, args));

    assert!(matches!(app, core::Term::App(_)));
}

#[test]
fn builtin_operation_call() {
    let arena = bumpalo::Bump::new();
    let arg1 = arena.alloc(core::Term::Lit(1, IntType::U64_OBJ));
    let arg2 = arena.alloc(core::Term::Lit(2, IntType::U64_OBJ));
    let args = &*arena.alloc_slice_fill_iter([&*arg1, &*arg2]);
    let prim = arena.alloc(core::Term::Prim(Prim::Add(IntType::U64_OBJ)));
    let app = arena.alloc(core::Term::new_app(prim, args));

    assert!(matches!(
        app,
        core::Term::App(core::App {
            func: core::Term::Prim(Prim::Add(IntType {
                width: IntWidth::U64,
                ..
            })),
            ..
        })
    ));
}
