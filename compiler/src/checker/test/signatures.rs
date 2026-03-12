//! Signatures and Elaboration Tests

use super::*;

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

    let functions = src_arena.alloc_slice_fill_iter([ast::Function {
        phase: Phase::Object,
        name: ast::Name::new("bad"),
        params: &[],
        ret_ty: lifted_ret,
        body,
    }]);
    let program = ast::Program { functions };

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

    let functions = src_arena.alloc_slice_fill_iter([ast::Function {
        phase: Phase::Object,
        name: ast::Name::new("bad"),
        params,
        ret_ty,
        body,
    }]);
    let program = ast::Program { functions };

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

    let functions = src_arena.alloc_slice_fill_iter([ast::Function {
        phase: Phase::Meta,
        name: ast::Name::new("bad"),
        params: &[],
        ret_ty,
        body,
    }]);
    let program = ast::Program { functions };

    assert!(
        super::collect_signatures(&core_arena, &program).is_err(),
        "`VmType` in meta-phase function return type should fail"
    );
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
