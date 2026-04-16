//! Signatures and Elaboration Tests

use super::*;

// Helper: build a desugared function `GlobalDef` from params/ret_ty/body.
// Mirrors what the parser produces for `def name(params) -> ret_ty = body;`.
fn make_fn_def<'names, 'ast>(
    arena: &'ast bumpalo::Bump,
    phase: Phase,
    name: &'names ast::Name,
    params: &'ast [ast::Param<'names, 'ast>],
    ret_ty: &'ast ast::Term<'names, 'ast>,
    body: &'ast ast::Term<'names, 'ast>,
) -> ast::GlobalDef<'names, 'ast> {
    let ty = arena.alloc(ast::Term::Pi { params, ret_ty });
    // Mirror parser desugaring: meta defs wrap body in Lam; object defs do not.
    let expr = if phase.is_meta() {
        arena.alloc(ast::Term::Lam {
            params,
            ret_ty: Some(ret_ty),
            body,
        })
    } else {
        body
    };
    ast::GlobalDef { phase, name, ty, expr }
}

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

    let defs = src_arena.alloc_slice_fill_iter([
        make_fn_def(
            &src_arena,
            Phase::Meta,
            ast::Name::new("id"),
            id_params,
            id_ret_ty,
            id_body,
        ),
        make_fn_def(
            &src_arena,
            Phase::Object,
            ast::Name::new("add_one"),
            add_params,
            add_ret_ty,
            add_body,
        ),
    ]);
    let program = ast::Program { defs };

    let globals = super::collect_signatures(&core_arena, &program)
        .expect("collect_signatures should succeed");

    assert_eq!(globals.len(), 2);

    let id_ty = globals
        .get(&Name::new("id"))
        .expect("id should be in globals");
    let id_pi = match id_ty {
        core::Term::Pi(pi) => pi,
        _ => panic!("id should have a Pi type"),
    };
    assert_eq!(id_pi.phase, Phase::Meta);
    assert_eq!(id_pi.params.len(), 1);
    assert_eq!(id_pi.params[0].0.as_str(), "x");
    assert!(matches!(
        id_pi.params[0].1,
        core::Term::Prim(Prim::IntTy(IntType {
            width: IntWidth::U32,
            ..
        }))
    ));
    assert!(matches!(
        id_pi.body_ty,
        core::Term::Prim(Prim::IntTy(IntType {
            width: IntWidth::U32,
            ..
        }))
    ));

    let add_ty = globals
        .get(&Name::new("add_one"))
        .expect("add_one should be in globals");
    let add_pi = match add_ty {
        core::Term::Pi(pi) => pi,
        _ => panic!("add_one should have a Pi type"),
    };
    assert_eq!(add_pi.phase, Phase::Object);
    assert_eq!(add_pi.params.len(), 1);
    assert_eq!(add_pi.params[0].0.as_str(), "y");
    assert!(matches!(
        add_pi.params[0].1,
        core::Term::Prim(Prim::IntTy(IntType {
            width: IntWidth::U64,
            ..
        }))
    ));
    assert!(matches!(
        add_pi.body_ty,
        core::Term::Prim(Prim::IntTy(IntType {
            width: IntWidth::U64,
            ..
        }))
    ));
}

// `[[T]]` as a return type annotation in an object-phase (`code fn`) function must fail.
#[test]
fn collect_signatures_lift_in_object_fn_fails() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();

    // `code fn bad() -> [[u64]] { ... }` — object-level function cannot have [[T]] as return type.
    let lifted_ret = src_arena.alloc(ast::Term::Lift(
        src_arena.alloc(ast::Term::Var(ast::Name::new("u64"))),
    ));
    let body = src_arena.alloc(ast::Term::Lit(0));

    let defs = src_arena.alloc_slice_fill_iter([make_fn_def(
        &src_arena,
        Phase::Object,
        ast::Name::new("bad"),
        &[],
        lifted_ret,
        body,
    )]);
    let program = ast::Program { defs };

    assert!(
        super::collect_signatures(&core_arena, &program).is_err(),
        "[[T]] in object-phase function signature should fail"
    );
}

// `Type` as a parameter type in a `code fn` is a meta type in an object context — must fail.
#[test]
fn collect_signatures_type_universe_in_object_fn_fails() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();

    // `code fn bad(x: Type) -> u64` — `Type` is meta-phase, illegal as object-fn param.
    let type_ann = src_arena.alloc(ast::Term::Var(ast::Name::new("Type")));
    let ret_ty = src_arena.alloc(ast::Term::Var(ast::Name::new("u64")));
    let body = src_arena.alloc(ast::Term::Lit(0));
    let params = src_arena.alloc_slice_fill_iter([ast::Param {
        name: ast::Name::new("x"),
        ty: type_ann,
    }]);

    let defs = src_arena.alloc_slice_fill_iter([make_fn_def(
        &src_arena,
        Phase::Object,
        ast::Name::new("bad"),
        params,
        ret_ty,
        body,
    )]);
    let program = ast::Program { defs };

    assert!(
        super::collect_signatures(&core_arena, &program).is_err(),
        "`Type` in object-phase function param should fail"
    );
}

// `VmType` as a return type in a meta `fn` is an object-universe type in meta context — must fail.
#[test]
fn collect_signatures_vmtype_in_meta_fn_fails() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();

    // `fn bad() -> VmType` — `VmType` is object-phase, illegal as meta-fn return type.
    let ret_ty = src_arena.alloc(ast::Term::Var(ast::Name::new("VmType")));
    let body = src_arena.alloc(ast::Term::Lit(0));

    let defs = src_arena.alloc_slice_fill_iter([make_fn_def(
        &src_arena,
        Phase::Meta,
        ast::Name::new("bad"),
        &[],
        ret_ty,
        body,
    )]);
    let program = ast::Program { defs };

    assert!(
        super::collect_signatures(&core_arena, &program).is_err(),
        "`VmType` in meta-phase function return type should fail"
    );
}

// Two definitions with the same name must produce an error.
#[test]
fn collect_signatures_duplicate_name_fails() {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();

    let ret_ty = src_arena.alloc(ast::Term::Var(ast::Name::new("u32")));
    let body = src_arena.alloc(ast::Term::Lit(0));

    let defs = src_arena.alloc_slice_fill_iter([
        make_fn_def(
            &src_arena,
            Phase::Meta,
            ast::Name::new("foo"),
            &[],
            ret_ty,
            body,
        ),
        make_fn_def(
            &src_arena,
            Phase::Meta,
            ast::Name::new("foo"),
            &[],
            ret_ty,
            body,
        ),
    ]);
    let program = ast::Program { defs };

    assert!(
        super::collect_signatures(&core_arena, &program).is_err(),
        "duplicate definition name should fail"
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
    let defs = src_arena.alloc_slice_fill_iter([make_fn_def(
        &src_arena,
        Phase::Meta,
        ast::Name::new("id"),
        param,
        ret_ty,
        body,
    )]);
    let program = ast::Program { defs };

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
        func: FunName::Term(src_arena.alloc(ast::Term::Var(ast::Name::new("k")))),
        args: &[],
    });
    let pow0_body = src_arena.alloc(ast::Term::Splice(k_call));
    let x_param = src_arena.alloc_slice_fill_iter([ast::Param {
        name: ast::Name::new("x"),
        ty: src_arena.alloc(ast::Term::Var(ast::Name::new("u64"))),
    }]);
    let pow0_ret = src_arena.alloc(ast::Term::Var(ast::Name::new("u64")));

    let defs = src_arena.alloc_slice_fill_iter([
        make_fn_def(
            &src_arena,
            Phase::Meta,
            ast::Name::new("k"),
            &[],
            k_ret,
            k_body,
        ),
        make_fn_def(
            &src_arena,
            Phase::Object,
            ast::Name::new("pow0"),
            x_param,
            pow0_ret,
            pow0_body,
        ),
    ]);
    let program = ast::Program { defs };

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
        func: FunName::Term(src_arena.alloc(ast::Term::Var(ast::Name::new("b")))),
        args: &[],
    });
    // fn b() -> u32 { 42 }
    let b_body = src_arena.alloc(ast::Term::Lit(42));

    let ret_ty = src_arena.alloc(ast::Term::Var(ast::Name::new("u32")));
    let defs = src_arena.alloc_slice_fill_iter([
        make_fn_def(
            &src_arena,
            Phase::Meta,
            ast::Name::new("a"),
            &[],
            ret_ty,
            a_body,
        ),
        make_fn_def(
            &src_arena,
            Phase::Meta,
            ast::Name::new("b"),
            &[],
            ret_ty,
            b_body,
        ),
    ]);
    let program = ast::Program { defs };

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

    let defs = src_arena.alloc_slice_fill_iter([make_fn_def(
        &src_arena,
        Phase::Meta,
        ast::Name::new("bad"),
        param,
        u32_ret,
        body,
    )]);
    let program = ast::Program { defs };

    assert!(elaborate_program(&core_arena, &program).is_err());
}
