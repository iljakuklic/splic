//! Checker behaviour tests — App { Global & Prim }

use super::*;

// Calling a known zero-argument global infers its return type.
#[test]
fn infer_global_call_no_args_returns_ret_ty() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut globals = HashMap::new();
    globals.insert(Name::new("f"), sig_no_params_returns_u64());
    let mut ctx = test_ctx_with_globals(&core_arena, &globals);

    let term = src_arena.alloc(ast::Term::App {
        func: FunName::Name(ast::Name::new("f")),
        args: &[],
    });
    let result = infer(&mut ctx, Phase::Meta, term).expect("should infer");
    let ty = ctx.type_of(result);
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
    globals.insert(Name::new("f"), sig_no_params_returns_u64());
    let mut ctx = test_ctx_with_globals(&core_arena, &globals);

    let term = src_arena.alloc(ast::Term::App {
        func: FunName::Name(ast::Name::new("f")),
        args,
    });
    assert!(infer(&mut ctx, Phase::Meta, term).is_err());
}

// Calling an object-phase global from a meta-phase context must fail.
#[test]
fn infer_global_call_phase_mismatch_fails() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();

    // `code fn f() -> u64` — object-phase function
    let u64_obj = core_arena.alloc(core::Term::Prim(Prim::IntTy(IntType::U64_OBJ)));
    let mut globals = HashMap::new();
    globals.insert(
        Name::new("f"),
        FunSig {
            params: &[],
            ret_ty: u64_obj,
            phase: Phase::Object,
        },
    );
    let mut ctx = test_ctx_with_globals(&core_arena, &globals);

    // Call `f()` from meta phase — should be rejected.
    let term = src_arena.alloc(ast::Term::App {
        func: FunName::Name(ast::Name::new("f")),
        args: &[],
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
    globals.insert(Name::new("f"), sig_one_param_returns_u64(&core_arena));
    let mut ctx = test_ctx_with_globals(&core_arena, &globals);

    let arg = src_arena.alloc(ast::Term::Lit(42));
    let args = src_arena.alloc_slice_fill_iter([arg as &ast::Term]);
    let term = src_arena.alloc(ast::Term::App {
        func: FunName::Name(ast::Name::new("f")),
        args,
    });
    let result = infer(&mut ctx, Phase::Meta, term).expect("should infer");
    let ty = ctx.type_of(result);
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
    let u32_obj = core::Term::int_ty(IntWidth::U32, Phase::Object);
    // push two object-phase u32 locals to use as operands
    ctx.push_local("a", u32_obj);
    ctx.push_local("b", u32_obj);

    let a = src_arena.alloc(ast::Term::Var(ast::Name::new("a")));
    let b = src_arena.alloc(ast::Term::Var(ast::Name::new("b")));
    let args = src_arena.alloc_slice_fill_iter([a as &ast::Term, b as &ast::Term]);
    let term = src_arena.alloc(ast::Term::App {
        func: FunName::BinOp(BinOp::Add),
        args,
    });

    let expected = core::Term::int_ty(IntWidth::U32, Phase::Object);
    let result = check(&mut ctx, Phase::Object, term, expected).expect("should check");
    assert!(matches!(
        result,
        core::Term::PrimApp(core::PrimApp {
            prim: Prim::Add(IntType {
                width: IntWidth::U32,
                ..
            }),
            ..
        })
    ));
}

// Comparison ops always return u1, so they are inferable without a type annotation.
#[test]
fn infer_comparison_op_returns_u1() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);
    let u64_obj = core::Term::int_ty(IntWidth::U64, Phase::Object);
    ctx.push_local("a", u64_obj);
    ctx.push_local("b", u64_obj);

    let a = src_arena.alloc(ast::Term::Var(ast::Name::new("a")));
    let b = src_arena.alloc(ast::Term::Var(ast::Name::new("b")));
    let args = src_arena.alloc_slice_fill_iter([a as &ast::Term, b as &ast::Term]);
    let term = src_arena.alloc(ast::Term::App {
        func: FunName::BinOp(BinOp::Eq),
        args,
    });

    // Eq is inferable: result is u1, prim carries the operand type (u64).
    let core_term = infer(&mut ctx, Phase::Object, term).expect("should infer");
    let ty = ctx.type_of(core_term);
    assert!(matches!(
        core_term,
        core::Term::PrimApp(core::PrimApp {
            prim: Prim::Eq(IntType {
                width: IntWidth::U64,
                ..
            }),
            ..
        })
    ));
    assert!(matches!(
        ty,
        core::Term::Prim(Prim::IntTy(IntType {
            width: IntWidth::U1,
            ..
        }))
    ));
}

// Comparison operands must have matching types: a: u64, b: u32 must fail.
#[test]
fn infer_comparison_op_mismatched_operands_fails() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);
    let u64_ty = &core::Term::U64_META;
    let u32_ty = &core::Term::U32_META;
    ctx.push_local("a", u64_ty);
    ctx.push_local("b", u32_ty); // different type

    let a = src_arena.alloc(ast::Term::Var(ast::Name::new("a")));
    let b = src_arena.alloc(ast::Term::Var(ast::Name::new("b")));
    let args = src_arena.alloc_slice_fill_iter([a as &ast::Term, b as &ast::Term]);
    let term = src_arena.alloc(ast::Term::App {
        func: FunName::BinOp(BinOp::Lt),
        args,
    });

    assert!(infer(&mut ctx, Phase::Object, term).is_err());
}

// `infer` on a bare binary op application (without expected type) must fail.
#[test]
fn infer_binop_add_without_expected_type_fails() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);
    let u32_ty = &core::Term::U32_META;
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
    let u64_ty = &core::Term::U64_META;
    let u32_ty = &core::Term::U32_META;
    ctx.push_local("a", u64_ty); // u64, but op expects u32
    ctx.push_local("b", u32_ty);

    let a = src_arena.alloc(ast::Term::Var(ast::Name::new("a")));
    let b = src_arena.alloc(ast::Term::Var(ast::Name::new("b")));
    let args = src_arena.alloc_slice_fill_iter([a as &ast::Term, b as &ast::Term]);
    let term = src_arena.alloc(ast::Term::App {
        func: FunName::BinOp(BinOp::Add),
        args,
    });

    let expected = &core::Term::U32_META; // we expect u32, but `a` is u64
    assert!(check(&mut ctx, Phase::Object, term, expected).is_err());
}

// A comparison `==` always produces u1, regardless of operand width.
// Checking it against u1 at the meta phase must succeed; the prim carries the operand type.
#[test]
fn check_eq_op_produces_u1() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);
    // Use meta-phase locals so the phase is consistent throughout.
    let u64_ty = &core::Term::U64_META; // u64 at meta phase
    ctx.push_local("a", u64_ty);
    ctx.push_local("b", u64_ty);

    let a = src_arena.alloc(ast::Term::Var(ast::Name::new("a")));
    let b = src_arena.alloc(ast::Term::Var(ast::Name::new("b")));
    let args = src_arena.alloc_slice_fill_iter([a as &ast::Term, b as &ast::Term]);
    let term = src_arena.alloc(ast::Term::App {
        func: FunName::BinOp(BinOp::Eq),
        args,
    });

    // Checking at meta phase against meta-phase u1.
    let expected = &core::Term::U1_META;
    let result = check(&mut ctx, Phase::Meta, term, expected).expect("should check");
    // The prim carries the operand type (u64), not u1.
    assert!(matches!(
        result,
        core::Term::PrimApp(core::PrimApp {
            prim: Prim::Eq(IntType {
                width: IntWidth::U64,
                ..
            }),
            ..
        })
    ));
}
