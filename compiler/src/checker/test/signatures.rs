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

    let id_ty = src_arena.alloc(ast::Term::Pi {
        params: id_params,
        ret_ty: id_ret_ty,
    });
    let id_body_lam = src_arena.alloc(ast::Term::Lam {
        params: id_params,
        ret_ty: None,
        body: id_body,
    });
    let defs = src_arena.alloc_slice_fill_iter([
        ast::GlobalDef::Meta(ast::MetaDef {
            name: ast::Name::new("id"),
            ty: id_ty,
            body: id_body_lam,
        }),
        ast::GlobalDef::Code(ast::CodeDef {
            name: ast::Name::new("add_one"),
            params: add_params,
            ret_ty: add_ret_ty,
            body: add_body,
        }),
    ]);
    let program = ast::Program { defs };

    let globals = super::collect_signatures(&core_arena, &program)
        .expect("collect_signatures should succeed");

    assert_eq!(globals.len(), 2);

    let GlobalEntry::Meta(id_ty) = globals
        .get(&Name::new("id"))
        .expect("id should be in globals")
    else {
        panic!("id should be a meta entry")
    };
    let core::Term::Pi(pi) = id_ty else {
        panic!("id should have Pi type")
    };
    assert_eq!(pi.params.len(), 1);
    assert_eq!(pi.params[0].0.as_str(), "x");
    assert!(matches!(
        pi.params[0].1,
        core::Term::Prim(Prim::IntTy(IntType {
            width: IntWidth::U32,
            ..
        }))
    ));
    assert!(matches!(
        pi.body_ty,
        core::Term::Prim(Prim::IntTy(IntType {
            width: IntWidth::U32,
            ..
        }))
    ));

    let GlobalEntry::CodeFn { params, ret_ty } = globals
        .get(&Name::new("add_one"))
        .expect("add_one should be in globals")
    else {
        panic!("add_one should be a CodeFn entry")
    };
    assert_eq!(params.len(), 1);
    assert_eq!(params[0].0.as_str(), "y");
    assert!(matches!(
        params[0].1,
        core::Term::Prim(Prim::IntTy(IntType {
            width: IntWidth::U64,
            ..
        }))
    ));
    assert!(matches!(
        ret_ty,
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

    let defs = src_arena.alloc_slice_fill_iter([ast::GlobalDef::Code(ast::CodeDef {
        name: ast::Name::new("bad"),
        params: &[],
        ret_ty: lifted_ret,
        body,
    })]);
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

    let defs = src_arena.alloc_slice_fill_iter([ast::GlobalDef::Code(ast::CodeDef {
        name: ast::Name::new("bad"),
        params,
        ret_ty,
        body,
    })]);
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

    let ty = src_arena.alloc(ast::Term::Pi {
        params: &[],
        ret_ty,
    });
    let body_lam = src_arena.alloc(ast::Term::Lam {
        params: &[],
        ret_ty: None,
        body,
    });
    let defs = src_arena.alloc_slice_fill_iter([ast::GlobalDef::Meta(ast::MetaDef {
        name: ast::Name::new("bad"),
        ty,
        body: body_lam,
    })]);
    let program = ast::Program { defs };

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

    let ty = src_arena.alloc(ast::Term::Pi {
        params: &[],
        ret_ty,
    });
    let body_lam = src_arena.alloc(ast::Term::Lam {
        params: &[],
        ret_ty: None,
        body,
    });
    let defs = src_arena.alloc_slice_fill_iter([
        ast::GlobalDef::Meta(ast::MetaDef {
            name: ast::Name::new("foo"),
            ty,
            body: body_lam,
        }),
        ast::GlobalDef::Meta(ast::MetaDef {
            name: ast::Name::new("foo"),
            ty,
            body: body_lam,
        }),
    ]);
    let program = ast::Program { defs };

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
    let ty = src_arena.alloc(ast::Term::Pi {
        params: param,
        ret_ty,
    });
    let body_lam = src_arena.alloc(ast::Term::Lam {
        params: param,
        ret_ty: None,
        body,
    });
    let defs = src_arena.alloc_slice_fill_iter([ast::GlobalDef::Meta(ast::MetaDef {
        name: ast::Name::new("id"),
        ty,
        body: body_lam,
    })]);
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

    let k_ty = src_arena.alloc(ast::Term::Pi {
        params: &[],
        ret_ty: k_ret,
    });
    let k_body_lam = src_arena.alloc(ast::Term::Lam {
        params: &[],
        ret_ty: None,
        body: k_body,
    });
    let defs = src_arena.alloc_slice_fill_iter([
        ast::GlobalDef::Meta(ast::MetaDef {
            name: ast::Name::new("k"),
            ty: k_ty,
            body: k_body_lam,
        }),
        ast::GlobalDef::Code(ast::CodeDef {
            name: ast::Name::new("pow0"),
            params: x_param,
            ret_ty: pow0_ret,
            body: pow0_body,
        }),
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

    let u32_ty = src_arena.alloc(ast::Term::Var(ast::Name::new("u32")));
    let a_ty = src_arena.alloc(ast::Term::Pi {
        params: &[],
        ret_ty: u32_ty,
    });
    let a_body_lam = src_arena.alloc(ast::Term::Lam {
        params: &[],
        ret_ty: None,
        body: a_body,
    });
    let b_ty = src_arena.alloc(ast::Term::Pi {
        params: &[],
        ret_ty: u32_ty,
    });
    let b_body_lam = src_arena.alloc(ast::Term::Lam {
        params: &[],
        ret_ty: None,
        body: b_body,
    });
    let defs = src_arena.alloc_slice_fill_iter([
        ast::GlobalDef::Meta(ast::MetaDef {
            name: ast::Name::new("a"),
            ty: a_ty,
            body: a_body_lam,
        }),
        ast::GlobalDef::Meta(ast::MetaDef {
            name: ast::Name::new("b"),
            ty: b_ty,
            body: b_body_lam,
        }),
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

    let ty = src_arena.alloc(ast::Term::Pi {
        params: param,
        ret_ty: u32_ret,
    });
    let body_lam = src_arena.alloc(ast::Term::Lam {
        params: param,
        ret_ty: None,
        body,
    });
    let defs = src_arena.alloc_slice_fill_iter([ast::GlobalDef::Meta(ast::MetaDef {
        name: ast::Name::new("bad"),
        ty,
        body: body_lam,
    })]);
    let program = ast::Program { defs };

    assert!(elaborate_program(&core_arena, &program).is_err());
}
