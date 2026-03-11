use std::collections::HashMap;

use super::*;

mod snap;
use crate::core::{self, FunSig, Head, IntType, IntWidth, Pat, Prim};
use crate::parser::ast::{self, BinOp, FunName, MatchArm, Phase};

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// Helper to create a test context with empty globals
fn test_ctx(arena: &bumpalo::Bump) -> Ctx<'_, '_> {
    static EMPTY: std::sync::OnceLock<HashMap<&'static str, core::FunSig<'static>>> =
        std::sync::OnceLock::new();
    let globals = EMPTY.get_or_init(HashMap::new);
    Ctx::new(arena, globals)
}

/// Helper to create a test context with a given globals table.
///
/// The caller must ensure `globals` outlives the returned `Ctx`.
fn test_ctx_with_globals<'core, 'globals>(
    arena: &'core bumpalo::Bump,
    globals: &'globals HashMap<&'core str, core::FunSig<'core>>,
) -> Ctx<'core, 'globals> {
    Ctx::new(arena, globals)
}

// ---------------------------------------------------------------------------
// Structural / context tests (these already pass; kept as regression guards)
// ---------------------------------------------------------------------------

#[test]
fn test_prim_types_are_well_kinded() {
    let arena = bumpalo::Bump::new();
    let ctx = test_ctx(&arena);
    let u64_term = ctx.u64_ty();
    assert!(matches!(
        u64_term,
        core::Term::Prim(Prim::IntTy(IntType {
            width: IntWidth::U64,
            ..
        }))
    ));
}

#[test]
fn test_literal_checks_against_int_type() {
    let arena = bumpalo::Bump::new();
    let lit = arena.alloc(core::Term::Lit(42));
    assert!(matches!(lit, core::Term::Lit(42)));
}

#[test]
fn test_variable_lookup_in_empty_context() {
    let arena = bumpalo::Bump::new();
    let ctx = test_ctx(&arena);
    assert_eq!(ctx.lookup_local("x"), None);
}

#[test]
fn test_variable_lookup_after_push() {
    let arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&arena);
    let u64_term = ctx.u64_ty();
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
fn test_variable_lookup_with_multiple_locals() {
    let arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&arena);
    let u64_term = ctx.u64_ty();
    let u32_term = ctx.u32_ty();

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
fn test_variable_shadowing() {
    let arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&arena);
    let u64_term = ctx.u64_ty();
    let u32_term = ctx.u32_ty();

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
fn test_context_depth() {
    let arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&arena);
    let u64_term = ctx.u64_ty();

    assert_eq!(ctx.depth(), 0);
    ctx.push_local("x", u64_term);
    assert_eq!(ctx.depth(), 1);
    ctx.push_local("y", u64_term);
    assert_eq!(ctx.depth(), 2);
    ctx.pop_local();
    assert_eq!(ctx.depth(), 1);
}

#[test]
fn test_meta_variable_in_quote_is_ok() {
    let arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&arena);
    let u64_term = ctx.u64_ty();
    let lifted_u64 = ctx.lift_ty(u64_term);
    ctx.push_local("x", lifted_u64);
    let x_var = arena.alloc(core::Term::Var(Lvl(0)));
    assert!(matches!(x_var, core::Term::Var(Lvl(0))));
}

#[test]
fn test_object_variable_outside_quote_is_invalid() {
    let arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&arena);
    let u64_term = ctx.u64_ty();
    ctx.push_local("x", u64_term);
    assert_eq!(ctx.depth(), 1);
}

#[test]
fn test_phase_is_argument_not_context() {
    let arena = bumpalo::Bump::new();
    let ctx = test_ctx(&arena);
    assert_eq!(ctx.depth(), 0);
}

#[test]
fn test_type_universe_distinction() {
    let arena = bumpalo::Bump::new();
    let ctx = test_ctx(&arena);
    let type_tm = ctx.type_ty();
    let vm_type_tm = ctx.vm_type_ty();

    assert!(matches!(type_tm, core::Term::Prim(Prim::U(Phase::Meta))));
    assert!(matches!(
        vm_type_tm,
        core::Term::Prim(Prim::U(Phase::Object))
    ));
}

#[test]
fn test_arithmetic_requires_expected_type() {
    let add_u32 = Prim::Add(IntType::new(IntWidth::U32, Phase::Object));
    assert!(matches!(
        add_u32,
        Prim::Add(IntType {
            width: IntWidth::U32,
            ..
        })
    ));
}

#[test]
fn test_global_call_is_inferable() {
    let arena = bumpalo::Bump::new();
    let arg = arena.alloc(core::Term::Lit(1));
    let args = &*arena.alloc_slice_fill_iter([&*arg]);
    let app = arena.alloc(core::Term::App {
        head: Head::Global("foo"),
        args,
    });
    assert!(matches!(
        app,
        core::Term::App {
            head: Head::Global("foo"),
            ..
        }
    ));
}

#[test]
fn test_comparison_operation_returns_u1() {
    let eq_u64 = Prim::Eq(IntType::new(IntWidth::U64, Phase::Object));
    assert!(matches!(
        eq_u64,
        Prim::Eq(IntType {
            width: IntWidth::U64,
            ..
        })
    ));
}

#[test]
fn test_lift_type_structure() {
    let arena = bumpalo::Bump::new();
    let ctx = test_ctx(&arena);
    let u64_term = ctx.u64_ty();
    let lifted = ctx.lift_ty(u64_term);
    assert!(matches!(lifted, core::Term::Lift(_)));
}

#[test]
fn test_quote_inference_mirrors_inner() {
    let arena = bumpalo::Bump::new();
    let inner = arena.alloc(core::Term::App {
        head: Head::Global("foo"),
        args: &*arena.alloc_slice_fill_iter([] as [&core::Term; 0]),
    });
    let quoted = arena.alloc(core::Term::Quote(inner));
    assert!(matches!(quoted, core::Term::Quote(_)));
}

#[test]
fn test_splice_inference_mirrors_inner() {
    let arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&arena);
    let u64_term = ctx.u64_ty();
    let lifted_u64 = ctx.lift_ty(u64_term);
    ctx.push_local("x", lifted_u64);
    let x_var = arena.alloc(core::Term::Var(Lvl(0)));
    let spliced = arena.alloc(core::Term::Splice(x_var));
    assert!(matches!(spliced, core::Term::Splice(_)));
}

#[test]
fn test_let_binding_structure() {
    let arena = bumpalo::Bump::new();
    let ctx = test_ctx(&arena);
    let u64_term = ctx.u64_ty();
    let expr = arena.alloc(core::Term::Lit(42));
    let body = arena.alloc(core::Term::Var(Lvl(0)));
    let let_term = arena.alloc(core::Term::Let {
        name: "x",
        ty: u64_term,
        expr,
        body,
    });
    assert!(matches!(let_term, core::Term::Let { .. }));
}

#[test]
fn test_match_with_literal_pattern() {
    let arena = bumpalo::Bump::new();
    let scrutinee = arena.alloc(core::Term::Var(Lvl(0)));
    let body0 = arena.alloc(core::Term::Lit(0));
    let body1 = arena.alloc(core::Term::Lit(1));

    let arm0 = core::Arm {
        pat: Pat::Lit(0),
        body: body0,
    };
    let arm1 = core::Arm {
        pat: Pat::Lit(1),
        body: body1,
    };

    let arms = &*arena.alloc_slice_fill_iter([arm0, arm1]);
    let match_term = arena.alloc(core::Term::Match { scrutinee, arms });

    assert!(matches!(match_term, core::Term::Match { .. }));
}

#[test]
fn test_match_with_binding_pattern() {
    let arena = bumpalo::Bump::new();
    let scrutinee = arena.alloc(core::Term::Var(Lvl(0)));
    let body = arena.alloc(core::Term::Var(Lvl(0)));

    let arm = core::Arm {
        pat: Pat::Bind("n"),
        body,
    };

    let arms = &*arena.alloc_slice_fill_iter([arm]);
    let match_term = arena.alloc(core::Term::Match { scrutinee, arms });

    assert!(matches!(match_term, core::Term::Match { .. }));
}

#[test]
fn test_function_call_to_global() {
    let arena = bumpalo::Bump::new();
    let arg = arena.alloc(core::Term::Lit(42));
    let args = &*arena.alloc_slice_fill_iter([&*arg]);
    let app = arena.alloc(core::Term::App {
        head: Head::Global("foo"),
        args,
    });

    assert!(matches!(
        app,
        core::Term::App {
            head: Head::Global("foo"),
            ..
        }
    ));
}

#[test]
fn test_builtin_operation_call() {
    let arena = bumpalo::Bump::new();
    let arg1 = arena.alloc(core::Term::Lit(1));
    let arg2 = arena.alloc(core::Term::Lit(2));
    let args = &*arena.alloc_slice_fill_iter([&*arg1, &*arg2]);
    let app = arena.alloc(core::Term::App {
        head: Head::Prim(Prim::Add(IntType::new(IntWidth::U64, Phase::Object))),
        args,
    });

    assert!(matches!(
        app,
        core::Term::App {
            head: Head::Prim(Prim::Add(IntType {
                width: IntWidth::U64,
                ..
            })),
            ..
        }
    ));
}

// ---------------------------------------------------------------------------
// Checker behaviour tests — Var
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Checker behaviour tests — Lit
// ---------------------------------------------------------------------------

// `check` a literal against its declared type succeeds and produces `Lit`.
#[test]
fn check_lit_against_matching_int_type_succeeds() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);
    let expected = ctx.u32_ty();

    let term = src_arena.alloc(ast::Term::Lit(42));
    let result = check(&mut ctx, Phase::Object, term, expected).expect("should check");
    assert!(matches!(result, core::Term::Lit(42)));
}

// `check` a literal against a non-integer type (universe) must fail.
#[test]
fn check_lit_against_universe_fails() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);
    let expected = ctx.type_ty(); // Type, not an integer type

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

// ---------------------------------------------------------------------------
// Checker behaviour tests — App { Global }
// ---------------------------------------------------------------------------

// Helper: build a simple FunSig for a function `fn f() -> u64` (no params, meta phase).
fn sig_no_params_returns_u64<'core>(core_arena: &'core bumpalo::Bump) -> FunSig<'core> {
    let ret_ty = core_arena.alloc(core::Term::Prim(Prim::IntTy(IntType::new(
        IntWidth::U64,
        Phase::Meta,
    ))));
    FunSig {
        params: &[],
        ret_ty,
        phase: Phase::Meta,
    }
}

// Helper: build a FunSig for `fn f(x: u32) -> u64`.
fn sig_one_param_returns_u64<'core>(core_arena: &'core bumpalo::Bump) -> FunSig<'core> {
    let u32_ty = core_arena.alloc(core::Term::Prim(Prim::IntTy(IntType::new(
        IntWidth::U32,
        Phase::Meta,
    ))));
    let u64_ty = core_arena.alloc(core::Term::Prim(Prim::IntTy(IntType::new(
        IntWidth::U64,
        Phase::Meta,
    ))));
    let param = core_arena.alloc(("x", u32_ty as &core::Term));
    FunSig {
        params: std::slice::from_ref(param),
        ret_ty: u64_ty,
        phase: Phase::Meta,
    }
}

// Calling a known zero-argument global infers its return type.
#[test]
fn infer_global_call_no_args_returns_ret_ty() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut globals = HashMap::new();
    globals.insert("f", sig_no_params_returns_u64(&core_arena));
    let mut ctx = test_ctx_with_globals(&core_arena, &globals);

    let term = src_arena.alloc(ast::Term::App {
        func: FunName::Name(ast::Name::new("f")),
        args: &[],
    });
    let (_, ty) = infer(&mut ctx, Phase::Meta, term).expect("should infer");
    assert!(matches!(
        ty,
        core::Term::Prim(Prim::IntTy(IntType {
            width: IntWidth::U64,
            ..
        }))
    ));
}

// Calling an unknown global must fail.
#[test]
fn infer_global_call_unknown_name_fails() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);

    let term = src_arena.alloc(ast::Term::App {
        func: FunName::Name(ast::Name::new("unknown")),
        args: &[],
    });
    assert!(infer(&mut ctx, Phase::Meta, term).is_err());
}

// Calling a global with the wrong number of arguments must fail.
#[test]
fn infer_global_call_wrong_arity_fails() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let extra_arg = src_arena.alloc(ast::Term::Lit(99));
    let args = src_arena.alloc_slice_fill_iter([extra_arg as &ast::Term]);
    let mut globals = HashMap::new();
    globals.insert("f", sig_no_params_returns_u64(&core_arena));
    let mut ctx = test_ctx_with_globals(&core_arena, &globals);

    let term = src_arena.alloc(ast::Term::App {
        func: FunName::Name(ast::Name::new("f")),
        args,
    });
    assert!(infer(&mut ctx, Phase::Meta, term).is_err());
}

// Calling a global with correct args type-checks arguments and infers the return type.
#[test]
fn infer_global_call_with_arg_checks_arg_type() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    // `f(x: u32) -> u64`; call `f(42u32)` — arg should be checked against u32
    let mut globals = HashMap::new();
    globals.insert("f", sig_one_param_returns_u64(&core_arena));
    let mut ctx = test_ctx_with_globals(&core_arena, &globals);

    let arg = src_arena.alloc(ast::Term::Lit(42));
    let args = src_arena.alloc_slice_fill_iter([arg as &ast::Term]);
    let term = src_arena.alloc(ast::Term::App {
        func: FunName::Name(ast::Name::new("f")),
        args,
    });
    let (_, ty) = infer(&mut ctx, Phase::Meta, term).expect("should infer");
    assert!(matches!(
        ty,
        core::Term::Prim(Prim::IntTy(IntType {
            width: IntWidth::U64,
            ..
        }))
    ));
}

// ---------------------------------------------------------------------------
// Checker behaviour tests — App { Prim } (BinOp / UnOp)
// ---------------------------------------------------------------------------

// `check` a binary op application against the expected integer type succeeds.
#[test]
fn check_binop_add_against_u32_succeeds() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);
    let u32_ty = ctx.u32_ty();
    // push two u32 locals to use as operands
    ctx.push_local("a", u32_ty);
    ctx.push_local("b", u32_ty);

    let a = src_arena.alloc(ast::Term::Var(ast::Name::new("a")));
    let b = src_arena.alloc(ast::Term::Var(ast::Name::new("b")));
    let args = src_arena.alloc_slice_fill_iter([a as &ast::Term, b as &ast::Term]);
    let term = src_arena.alloc(ast::Term::App {
        func: FunName::BinOp(BinOp::Add),
        args,
    });

    let expected = ctx.u32_ty();
    let result = check(&mut ctx, Phase::Object, term, expected).expect("should check");
    assert!(matches!(
        result,
        core::Term::App {
            head: Head::Prim(Prim::Add(IntType {
                width: IntWidth::U32,
                ..
            })),
            ..
        }
    ));
}

// `infer` on a bare binary op application (without expected type) must fail.
#[test]
fn infer_binop_add_without_expected_type_fails() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);
    let u32_ty = ctx.u32_ty();
    ctx.push_local("a", u32_ty);
    ctx.push_local("b", u32_ty);

    let a = src_arena.alloc(ast::Term::Var(ast::Name::new("a")));
    let b = src_arena.alloc(ast::Term::Var(ast::Name::new("b")));
    let args = src_arena.alloc_slice_fill_iter([a as &ast::Term, b as &ast::Term]);
    let term = src_arena.alloc(ast::Term::App {
        func: FunName::BinOp(BinOp::Add),
        args,
    });

    assert!(infer(&mut ctx, Phase::Object, term).is_err());
}

// Applying a binary op to arguments of the wrong type must fail.
#[test]
fn check_binop_add_with_mismatched_operand_types_fails() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);
    // push a (u64) and b (u32) — they don't match the expected u32 for 'a'
    let u64_ty = ctx.u64_ty();
    let u32_ty = ctx.u32_ty();
    ctx.push_local("a", u64_ty); // u64, but op expects u32
    ctx.push_local("b", u32_ty);

    let a = src_arena.alloc(ast::Term::Var(ast::Name::new("a")));
    let b = src_arena.alloc(ast::Term::Var(ast::Name::new("b")));
    let args = src_arena.alloc_slice_fill_iter([a as &ast::Term, b as &ast::Term]);
    let term = src_arena.alloc(ast::Term::App {
        func: FunName::BinOp(BinOp::Add),
        args,
    });

    let expected = ctx.u32_ty(); // we expect u32, but `a` is u64
    assert!(check(&mut ctx, Phase::Object, term, expected).is_err());
}

// A comparison `==` always produces u1, regardless of operand width.
// Checking it against VmType::U1 at the object phase must succeed.
#[test]
fn check_eq_op_produces_u1() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);
    let u64_ty = ctx.u64_ty();
    ctx.push_local("a", u64_ty);
    ctx.push_local("b", u64_ty);

    let a = src_arena.alloc(ast::Term::Var(ast::Name::new("a")));
    let b = src_arena.alloc(ast::Term::Var(ast::Name::new("b")));
    let args = src_arena.alloc_slice_fill_iter([a as &ast::Term, b as &ast::Term]);
    let term = src_arena.alloc(ast::Term::App {
        func: FunName::BinOp(BinOp::Eq),
        args,
    });

    let expected = ctx.u1_ty();
    let result = check(&mut ctx, Phase::Object, term, expected).expect("should check");
    assert!(matches!(
        result,
        core::Term::App {
            head: Head::Prim(Prim::Eq(_)),
            ..
        }
    ));
}

// ---------------------------------------------------------------------------
// Checker behaviour tests — Let
// ---------------------------------------------------------------------------

// `let x: u32 = 42; x` infers as u32.
#[test]
fn infer_let_annotated_infers_body_type() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);

    // Build: `let x: u32 = 42; x`
    let ty_ann = src_arena.alloc(ast::Term::Var(ast::Name::new("u32"))); // type annotation in surface AST
    let expr = src_arena.alloc(ast::Term::Lit(42));
    let body = src_arena.alloc(ast::Term::Var(ast::Name::new("x")));
    let stmts = src_arena.alloc_slice_fill_iter([ast::Let {
        name: ast::Name::new("x"),
        ty: Some(ty_ann),
        expr,
    }]);
    let block = src_arena.alloc(ast::Term::Block { stmts, expr: body });

    let (_, ty) = infer(&mut ctx, Phase::Meta, block).expect("should infer");
    assert!(matches!(
        ty,
        core::Term::Prim(Prim::IntTy(IntType {
            width: IntWidth::U32,
            ..
        }))
    ));
}

// `let x = 42; x` — no annotation, literal is uninferrable — must fail.
#[test]
fn infer_let_unannotated_uninferrable_expr_fails() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);

    let expr = src_arena.alloc(ast::Term::Lit(42)); // no type, can't infer
    let body = src_arena.alloc(ast::Term::Var(ast::Name::new("x")));
    let stmts = src_arena.alloc_slice_fill_iter([ast::Let {
        name: ast::Name::new("x"),
        ty: None,
        expr,
    }]);
    let block = src_arena.alloc(ast::Term::Block { stmts, expr: body });

    assert!(infer(&mut ctx, Phase::Meta, block).is_err());
}

// Let with annotation that doesn't match the expression type must fail.
#[test]
fn infer_let_annotation_mismatch_fails() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);
    let u64_ty = ctx.u64_ty();
    ctx.push_local("y", u64_ty); // y: u64

    // `let x: u32 = y; x`  — y is u64, annotation says u32
    let ty_ann = src_arena.alloc(ast::Term::Var(ast::Name::new("u32")));
    let expr = src_arena.alloc(ast::Term::Var(ast::Name::new("y")));
    let body = src_arena.alloc(ast::Term::Var(ast::Name::new("x")));
    let stmts = src_arena.alloc_slice_fill_iter([ast::Let {
        name: ast::Name::new("x"),
        ty: Some(ty_ann),
        expr,
    }]);
    let block = src_arena.alloc(ast::Term::Block { stmts, expr: body });

    assert!(infer(&mut ctx, Phase::Meta, block).is_err());
}

// ---------------------------------------------------------------------------
// Checker behaviour tests — Match
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Checker behaviour tests — Lift
// ---------------------------------------------------------------------------

// `[[u64]]` is a well-formed meta type: `infer` returns `Type`.
#[test]
fn infer_lift_of_object_type_returns_type_universe() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);

    // `[[u64]]` in surface AST: `Lift(Var("u64"))`
    let inner = src_arena.alloc(ast::Term::Var(ast::Name::new("u64")));
    let term = src_arena.alloc(ast::Term::Lift(inner));

    // Elaborated at meta phase: type of [[u64]] is Type (meta universe)
    let (_, ty) = infer(&mut ctx, Phase::Meta, term).expect("should infer");
    assert!(matches!(ty, core::Term::Prim(Prim::U(Phase::Meta))));
}

// Lifting a non-type value must fail.
#[test]
fn infer_lift_of_non_type_fails() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);

    // Push a local `x: u32` (a value, not a type) then write `[[x]]`
    let u32_ty = ctx.u32_ty();
    ctx.push_local("x", u32_ty);

    let inner = src_arena.alloc(ast::Term::Var(ast::Name::new("x")));
    let term = src_arena.alloc(ast::Term::Lift(inner));

    assert!(infer(&mut ctx, Phase::Meta, term).is_err());
}

// ---------------------------------------------------------------------------
// Checker behaviour tests — Quote
// ---------------------------------------------------------------------------

// `#(f())` where `f: () -> u64` at the object phase infers as `[[u64]]` at meta.
#[test]
fn infer_quote_of_global_call_returns_lifted_type() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();

    // `code fn f() -> u64` — object-phase function
    let u64_ty_core = core_arena.alloc(core::Term::Prim(Prim::IntTy(IntType::new(
        IntWidth::U64,
        Phase::Object,
    ))));
    let mut globals = HashMap::new();
    globals.insert(
        "f",
        FunSig {
            params: &[],
            ret_ty: u64_ty_core,
            phase: Phase::Object,
        },
    );
    let mut ctx = test_ctx_with_globals(&core_arena, &globals);

    // Surface: `#(f())`
    let inner = src_arena.alloc(ast::Term::App {
        func: FunName::Name(ast::Name::new("f")),
        args: &[],
    });
    let term = src_arena.alloc(ast::Term::Quote(inner));

    // Checked at meta phase; result type should be [[u64]]
    let (core_term, ty) = infer(&mut ctx, Phase::Meta, term).expect("should infer");
    assert!(matches!(core_term, core::Term::Quote(_)));
    // Type is Lift(u64)
    assert!(matches!(ty, core::Term::Lift(_)));
}

// `#(42)` — literal inside quote is not inferable, so the whole quote is not inferrable.
#[test]
fn infer_quote_of_literal_fails() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);

    let inner = src_arena.alloc(ast::Term::Lit(42));
    let term = src_arena.alloc(ast::Term::Quote(inner));

    assert!(infer(&mut ctx, Phase::Meta, term).is_err());
}

// Inside `#(...)` the phase is object, so a meta-only construct would be invalid.
// We can test this by verifying that checking `#(x)` where x: [[u64]] succeeds
// (meta var, used via splice), and that the inner term is indeed checked at object phase.
#[test]
fn check_quote_switches_to_object_phase() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);

    // x : [[u64]] — meta variable holding object code
    let u64_ty = ctx.u64_ty();
    let lifted = ctx.lift_ty(u64_ty);
    ctx.push_local("x", lifted);

    // `#($(x))` — splice x inside a quote; type should be [[u64]]
    let x = src_arena.alloc(ast::Term::Var(ast::Name::new("x")));
    let splice_x = src_arena.alloc(ast::Term::Splice(x));
    let term = src_arena.alloc(ast::Term::Quote(splice_x));

    // [[u64]] as the expected meta type
    let u64_ty_core = core_arena.alloc(core::Term::Prim(Prim::IntTy(IntType::new(
        IntWidth::U64,
        Phase::Meta,
    ))));
    let expected = core_arena.alloc(core::Term::Lift(u64_ty_core));

    let result = check(&mut ctx, Phase::Meta, term, expected).expect("should check");
    assert!(matches!(result, core::Term::Quote(_)));
}

// ---------------------------------------------------------------------------
// Checker behaviour tests — Splice
// ---------------------------------------------------------------------------

// `$(x)` where `x: [[u64]]` infers as `u64` at object phase.
#[test]
fn infer_splice_of_lifted_var_returns_inner_type() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);

    let u64_ty = ctx.u64_ty();
    let lifted = ctx.lift_ty(u64_ty);
    ctx.push_local("x", lifted); // x: [[u64]]

    let x = src_arena.alloc(ast::Term::Var(ast::Name::new("x")));
    let term = src_arena.alloc(ast::Term::Splice(x));

    // splice is checked at object phase
    let (core_term, ty) = infer(&mut ctx, Phase::Object, term).expect("should infer");
    assert!(matches!(core_term, core::Term::Splice(_)));
    assert!(matches!(
        ty,
        core::Term::Prim(Prim::IntTy(IntType {
            width: IntWidth::U64,
            ..
        }))
    ));
}

// `$(42)` — literal inside splice has no type, so infer fails.
#[test]
fn infer_splice_of_literal_fails() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);

    let inner = src_arena.alloc(ast::Term::Lit(42));
    let term = src_arena.alloc(ast::Term::Splice(inner));

    assert!(infer(&mut ctx, Phase::Object, term).is_err());
}

// `$(x)` where `x: u64` (not lifted) must fail — splice requires `[[T]]`.
#[test]
fn infer_splice_of_non_lifted_var_fails() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);

    let u64_ty = ctx.u64_ty();
    ctx.push_local("x", u64_ty); // x: u64, NOT [[u64]]

    let x = src_arena.alloc(ast::Term::Var(ast::Name::new("x")));
    let term = src_arena.alloc(ast::Term::Splice(x));

    assert!(infer(&mut ctx, Phase::Object, term).is_err());
}

// ---------------------------------------------------------------------------
// collect_signatures tests
// ---------------------------------------------------------------------------

// A program with two distinct functions produces a globals map with one entry per function,
// each carrying the correct param types, return type, and phase.
#[test]
fn collect_signatures_two_functions() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();

    // fn id(x: u32) -> u32 { x }
    let id_param_ty = src_arena.alloc(ast::Term::Var(ast::Name::new("u32")));
    let id_ret_ty = src_arena.alloc(ast::Term::Var(ast::Name::new("u32")));
    let id_body = src_arena.alloc(ast::Term::Var(ast::Name::new("x")));
    let id_params = src_arena.alloc_slice_fill_iter([ast::Param {
        name: ast::Name::new("x"),
        ty: id_param_ty,
    }]);

    // code fn add_one(y: u64) -> u64 { y }
    let add_param_ty = src_arena.alloc(ast::Term::Var(ast::Name::new("u64")));
    let add_ret_ty = src_arena.alloc(ast::Term::Var(ast::Name::new("u64")));
    let add_body = src_arena.alloc(ast::Term::Var(ast::Name::new("y")));
    let add_params = src_arena.alloc_slice_fill_iter([ast::Param {
        name: ast::Name::new("y"),
        ty: add_param_ty,
    }]);

    let functions = src_arena.alloc_slice_fill_iter([
        ast::Function {
            phase: Phase::Meta,
            name: ast::Name::new("id"),
            params: id_params,
            ret_ty: id_ret_ty,
            body: id_body,
        },
        ast::Function {
            phase: Phase::Object,
            name: ast::Name::new("add_one"),
            params: add_params,
            ret_ty: add_ret_ty,
            body: add_body,
        },
    ]);
    let program = ast::Program { functions };

    let globals = super::collect_signatures(&core_arena, &program)
        .expect("collect_signatures should succeed");

    assert_eq!(globals.len(), 2);

    let id_sig = globals.get("id").expect("id should be in globals");
    assert_eq!(id_sig.phase, Phase::Meta);
    assert_eq!(id_sig.params.len(), 1);
    assert_eq!(id_sig.params[0].0, "x");
    assert!(matches!(
        id_sig.params[0].1,
        core::Term::Prim(Prim::IntTy(IntType {
            width: IntWidth::U32,
            ..
        }))
    ));
    assert!(matches!(
        id_sig.ret_ty,
        core::Term::Prim(Prim::IntTy(IntType {
            width: IntWidth::U32,
            ..
        }))
    ));

    let add_sig = globals
        .get("add_one")
        .expect("add_one should be in globals");
    assert_eq!(add_sig.phase, Phase::Object);
    assert_eq!(add_sig.params.len(), 1);
    assert_eq!(add_sig.params[0].0, "y");
    assert!(matches!(
        add_sig.params[0].1,
        core::Term::Prim(Prim::IntTy(IntType {
            width: IntWidth::U64,
            ..
        }))
    ));
    assert!(matches!(
        add_sig.ret_ty,
        core::Term::Prim(Prim::IntTy(IntType {
            width: IntWidth::U64,
            ..
        }))
    ));
}

// Two functions with the same name must produce an error.
#[test]
fn collect_signatures_duplicate_name_fails() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();

    let ret_ty = src_arena.alloc(ast::Term::Var(ast::Name::new("u32")));
    let body = src_arena.alloc(ast::Term::Lit(0));

    let functions = src_arena.alloc_slice_fill_iter([
        ast::Function {
            phase: Phase::Meta,
            name: ast::Name::new("foo"),
            params: &[],
            ret_ty,
            body,
        },
        ast::Function {
            phase: Phase::Meta,
            name: ast::Name::new("foo"),
            params: &[],
            ret_ty,
            body,
        },
    ]);
    let program = ast::Program { functions };

    assert!(
        super::collect_signatures(&core_arena, &program).is_err(),
        "duplicate function name should fail"
    );
}

// ---------------------------------------------------------------------------
// Checker behaviour tests — elaborate_program
// ---------------------------------------------------------------------------

// A trivial well-typed meta function `fn id(x: u32) -> u32 { x }` elaborates successfully.
#[test]
fn elaborate_program_simple_identity_fn() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();

    let u32_ann = src_arena.alloc(ast::Term::Var(ast::Name::new("u32")));
    let param = src_arena.alloc_slice_fill_iter([ast::Param {
        name: ast::Name::new("x"),
        ty: u32_ann,
    }]);
    let ret_ty = src_arena.alloc(ast::Term::Var(ast::Name::new("u32")));
    let body = src_arena.alloc(ast::Term::Var(ast::Name::new("x")));
    let functions = src_arena.alloc_slice_fill_iter([ast::Function {
        phase: Phase::Meta,
        name: ast::Name::new("id"),
        params: param,
        ret_ty,
        body,
    }]);
    let program = ast::Program { functions };

    let result = elaborate_program(&core_arena, &program);
    assert!(result.is_ok());
}

// A `code fn` with a splice of a meta-result: `code fn pow0(x: u64) -> u64 { $(k()) }`.
// `k` is a meta fn returning `[[u64]]`.
#[test]
fn elaborate_program_code_fn_with_splice() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    // fn k() -> [[u64]] { #(0) }
    // code fn pow0(x: u64) -> u64 { $(k()) }

    // k's body: #(0)  — checks against [[u64]]
    let zero = src_arena.alloc(ast::Term::Lit(0));
    let k_body = src_arena.alloc(ast::Term::Quote(zero));
    let k_ret = src_arena.alloc(ast::Term::Lift(
        src_arena.alloc(ast::Term::Var(ast::Name::new("u64"))),
    ));

    // pow0's body: $(k())
    let k_call = src_arena.alloc(ast::Term::App {
        func: FunName::Name(ast::Name::new("k")),
        args: &[],
    });
    let pow0_body = src_arena.alloc(ast::Term::Splice(k_call));
    let x_param = src_arena.alloc_slice_fill_iter([ast::Param {
        name: ast::Name::new("x"),
        ty: src_arena.alloc(ast::Term::Var(ast::Name::new("u64"))),
    }]);
    let pow0_ret = src_arena.alloc(ast::Term::Var(ast::Name::new("u64")));

    let functions = src_arena.alloc_slice_fill_iter([
        ast::Function {
            phase: Phase::Meta,
            name: ast::Name::new("k"),
            params: &[],
            ret_ty: k_ret,
            body: k_body,
        },
        ast::Function {
            phase: Phase::Object,
            name: ast::Name::new("pow0"),
            params: x_param,
            ret_ty: pow0_ret,
            body: pow0_body,
        },
    ]);
    let program = ast::Program { functions };

    let result = elaborate_program(&core_arena, &program);
    assert!(result.is_ok());
}

// A forward reference: `fn a() -> u32 { b() }` / `fn b() -> u32 { 42 }` must succeed.
#[test]
fn elaborate_program_forward_reference_succeeds() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();

    // fn a() -> u32 { b() }
    let a_body = src_arena.alloc(ast::Term::App {
        func: FunName::Name(ast::Name::new("b")),
        args: &[],
    });
    // fn b() -> u32 { 42 }
    let b_body = src_arena.alloc(ast::Term::Lit(42));

    let functions = src_arena.alloc_slice_fill_iter([
        ast::Function {
            phase: Phase::Meta,
            name: ast::Name::new("a"),
            params: &[],
            ret_ty: src_arena.alloc(ast::Term::Var(ast::Name::new("u32"))),
            body: a_body,
        },
        ast::Function {
            phase: Phase::Meta,
            name: ast::Name::new("b"),
            params: &[],
            ret_ty: src_arena.alloc(ast::Term::Var(ast::Name::new("u32"))),
            body: b_body,
        },
    ]);
    let program = ast::Program { functions };

    let result = elaborate_program(&core_arena, &program);
    assert!(result.is_ok());
}

// A return type mismatch must fail: `fn bad() -> u32 { 42u64 }`.
#[test]
fn elaborate_program_return_type_mismatch_fails() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();

    // The body is a variable of type u64 but the declared return type is u32.
    // We express this by having a parameter `x: u64` and returning `x` when
    // the declared return is `u32`.
    let u64_ann = src_arena.alloc(ast::Term::Var(ast::Name::new("u64")));
    let u32_ret = src_arena.alloc(ast::Term::Var(ast::Name::new("u32")));
    let param = src_arena.alloc_slice_fill_iter([ast::Param {
        name: ast::Name::new("x"),
        ty: u64_ann,
    }]);
    let body = src_arena.alloc(ast::Term::Var(ast::Name::new("x"))); // x: u64, but ret says u32

    let functions = src_arena.alloc_slice_fill_iter([ast::Function {
        phase: Phase::Meta,
        name: ast::Name::new("bad"),
        params: param,
        ret_ty: u32_ret,
        body,
    }]);
    let program = ast::Program { functions };

    assert!(elaborate_program(&core_arena, &program).is_err());
}
