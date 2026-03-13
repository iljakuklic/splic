use std::path::PathBuf;

use expect_test::expect_file;
use rstest::rstest;

use crate::checker::elaborate_program;
use crate::eval::unstage_program;
use crate::lexer::Lexer;
use crate::parser::Parser;

fn stage(input: &str) -> String {
    let src_arena = bumpalo::Bump::new();
    let core_arena = bumpalo::Bump::new();
    let lexer = Lexer::new(input);
    let mut parser = Parser::new(lexer, &src_arena);
    let program = parser.parse_program().expect("parse error");
    let core_program = elaborate_program(&core_arena, &program).expect("elaboration error");
    let staged = unstage_program(&core_arena, &core_program).expect("staging error");
    format!("{staged:#?}\n")
}

#[rstest]
#[timeout(std::time::Duration::from_secs(5))]
fn stage_snap(#[files("src/eval/test/snap/*.input.txt")] path: PathBuf) {
    let input = std::fs::read_to_string(&path).unwrap();
    let actual = stage(&input);
    let snap_path = path.with_extension("").with_extension("snap.txt");
    expect_file![snap_path].assert_eq(&actual);
}
