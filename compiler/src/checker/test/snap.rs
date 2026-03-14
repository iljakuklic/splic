use std::path::PathBuf;

use expect_test::expect_file;
use rstest::rstest;

use crate::checker::elaborate_program;
use crate::lexer::Lexer;
use crate::parser::Parser;

fn elaborate(input: &str) -> String {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let lexer = Lexer::new(input);
    let mut parser = Parser::new(lexer, &src_arena);
    let program = parser.parse_program().expect("parse error");
    let core_program = elaborate_program(&core_arena, &program).expect("elaboration error");
    format!("{core_program}\n")
}

fn elaborate_err(input: &str) -> String {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let lexer = Lexer::new(input);
    let mut parser = Parser::new(lexer, &src_arena);
    let program = parser.parse_program().expect("parse error");
    let err = elaborate_program(&core_arena, &program).expect_err("expected elaboration error");
    format!("{err}\n")
}

#[rstest]
#[timeout(std::time::Duration::from_secs(1))]
fn elaborate_snap(#[files("src/checker/test/snap/*.input.txt")] path: PathBuf) {
    let input = std::fs::read_to_string(&path).unwrap();
    let actual = elaborate(&input);
    let snap_path = path.with_extension("").with_extension("snap.txt");
    expect_file![snap_path].assert_eq(&actual);
}

#[rstest]
#[timeout(std::time::Duration::from_secs(1))]
fn elaborate_err_snap(#[files("src/checker/test/snap/error/*.input.txt")] path: PathBuf) {
    let input = std::fs::read_to_string(&path).unwrap();
    let actual = elaborate_err(&input);
    let snap_path = path.with_extension("").with_extension("snap.txt");
    expect_file![snap_path].assert_eq(&actual);
}
