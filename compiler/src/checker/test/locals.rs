//! Checker behaviour tests — Let

use super::*;

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
    let stmts = src_arena.alloc_slice_fill_iter([ast::Definition {
        name: ast::Name::new("x"),
        params: &[],
        ret_ty: Some(ty_ann),
        body: expr,
    }]);
    let block = src_arena.alloc(ast::Term::Block { stmts, expr: body });

    let (_, ty_val) = infer(&mut ctx, Phase::Meta, block).expect("should infer");
    assert!(matches!(
        ty_val,
        value::Value::Prim(Prim::IntTy(IntType {
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
    let stmts = src_arena.alloc_slice_fill_iter([ast::Definition {
        name: ast::Name::new("x"),
        params: &[],
        ret_ty: None,
        body: expr,
    }]);
    let block = src_arena.alloc(ast::Term::Block { stmts, expr: body });

    assert!(infer(&mut ctx, Phase::Meta, block).is_err());
}

// A let binding with a `VmType` annotation in meta context must fail (wrong-phase type).
#[test]
fn infer_let_wrong_phase_annotation_fails() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let mut ctx = test_ctx(&core_arena);

    // `let x: VmType = ...; x` at meta phase — `VmType` is an object-phase universe, illegal here.
    let ty_ann = src_arena.alloc(ast::Term::Var(ast::Name::new("VmType")));
    let expr = src_arena.alloc(ast::Term::Lit(0));
    let body = src_arena.alloc(ast::Term::Var(ast::Name::new("x")));
    let stmts = src_arena.alloc_slice_fill_iter([ast::Definition {
        name: ast::Name::new("x"),
        params: &[],
        ret_ty: Some(ty_ann),
        body: expr,
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
    let u64_ty = &core::Term::U64_META;
    ctx.push_local(core::Name::new("y"), u64_ty); // y: u64

    // `let x: u32 = y; x`  — y is u64, annotation says u32
    let ty_ann = src_arena.alloc(ast::Term::Var(ast::Name::new("u32")));
    let expr = src_arena.alloc(ast::Term::Var(ast::Name::new("y")));
    let body = src_arena.alloc(ast::Term::Var(ast::Name::new("x")));
    let stmts = src_arena.alloc_slice_fill_iter([ast::Definition {
        name: ast::Name::new("x"),
        params: &[],
        ret_ty: Some(ty_ann),
        body: expr,
    }]);
    let block = src_arena.alloc(ast::Term::Block { stmts, expr: body });

    assert!(infer(&mut ctx, Phase::Meta, block).is_err());
}
