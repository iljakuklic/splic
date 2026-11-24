//! Strictness of the staging pass: staging runs meta-level code but performs no
//! object-level computation. Object redexes in the input — including ones produced
//! by splices — must survive verbatim.
//!
//! Each case pairs an input program with a splice-free `expected` program and
//! asserts that staging the input yields exactly the elaborated `expected` code
//! defs (up to alpha-equality). Constant folding or inlining, if ever wanted,
//! belongs in a separate pass after staging, not in the staging evaluator.

#![allow(clippy::unwrap_used, reason = "test code")]
#![allow(
    clippy::tests_outside_test_module,
    reason = "integration test binary under tests/ is already test-only"
)]

mod common;

use bumpalo::Bump;
use common::global_alpha_eq;
use rstest::rstest;
use splic_compiler::{
    checker::elaborate_program,
    core::{Global, Program},
    lexer::Lexer,
    parser::Parser,
    staging::unstage_program,
};

/// Run lex → parse → elaborate, panicking on any phase failure.
fn elaborate<'a>(arena: &'a Bump, src: &'a str) -> Program<'a, 'a> {
    let tokens: Vec<_> = Lexer::new(src, arena)
        .collect::<Result<_, _>>()
        .expect("lex");
    let program = Parser::new(tokens.into_iter().map(Ok), arena)
        .parse_program()
        .expect("parse");
    elaborate_program(arena, arena.alloc(program)).expect("elaborate")
}

#[rstest]
#[case::object_let(
    "code def f() -> u64 = { let x: u64 = 1; x };",
    "code def f() -> u64 = { let x: u64 = 1; x };"
)]
#[case::object_match_on_known_value(
    "code def f() -> u64 = { let s: u64 = 0; match s { 0 => 1, _ => 2 } };",
    "code def f() -> u64 = { let s: u64 = 0; match s { 0 => 1, _ => 2 } };"
)]
#[case::prim_on_literals("code def f() -> u64 = 1 + 1;", "code def f() -> u64 = 1 + 1;")]
#[case::redex_from_splice(
    "def two() -> [[u64]] = #(1 + 1);
     code def f() -> u64 = { $(two()) };",
    "code def f() -> u64 = 1 + 1;"
)]
#[case::let_of_spliced_code_not_inlined(
    "def two() -> [[u64]] = #(1 + 1);
     def dup(x: [[u64]]) -> [[u64]] = #($(x) + $(x));
     code def f() -> u64 = { let z: u64 = $(two()); $(dup(#(z))) };",
    "code def f() -> u64 = { let z: u64 = 1 + 1; z + z };"
)]
#[case::object_call_not_inlined(
    "def dup(x: [[u64]]) -> [[u64]] = #($(x) + $(x));
     code def c() -> u64 = 1 + 1;
     code def f() -> u64 = { $(dup(#(c()))) };",
    "code def c() -> u64 = 1 + 1;
     code def f() -> u64 = c() + c();"
)]
// Contrast: meta-level computation *is* performed — the meta call is unfolded,
// quoted code is duplicated, and meta arithmetic is computed — while the object
// addition in the duplicated code is left alone.
#[case::meta_computation_performed(
    "def dup(x: [[u64]]) -> [[u64]] = #($(x) + $(x));
     def n() -> u64 = 40 + 2;
     code def f() -> u64 = { $(dup(#(1 + 2))) };
     code def g() -> u64 = { $(n()) };",
    "code def f() -> u64 = (1 + 2) + (1 + 2);
     code def g() -> u64 = 42;"
)]
fn staging_is_strict(#[case] input: &str, #[case] expected: &str) {
    let arena = Bump::new();
    let staged = unstage_program(&arena, &elaborate(&arena, input)).expect("stage");
    let expected = elaborate(&arena, expected);
    assert!(
        expected
            .defs
            .iter()
            .all(|d| !matches!(d.global, Global::Meta(_))),
        "`expected` must contain only code defs"
    );

    assert_eq!(
        staged.defs.len(),
        expected.defs.len(),
        "staged:\n{staged}\nexpected:\n{expected}"
    );
    for (s, e) in staged.defs.iter().zip(expected.defs) {
        assert!(
            s.name == e.name && global_alpha_eq(&s.global, &e.global),
            "staged:\n{s}\nexpected:\n{e}"
        );
    }
}
