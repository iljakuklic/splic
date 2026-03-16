pub mod fuzz;

use crate::lexer::Lexer;
use expect_test::expect_file;
use rstest::rstest;
use std::path::PathBuf;

#[rstest]
#[timeout(std::time::Duration::from_secs(if cfg!(miri) { 600 } else { 5 }))] // catch infinite loops on invalid input
fn lex(#[files("src/lexer/test/lex/*.input.txt")] path: PathBuf) {
    let input = std::fs::read_to_string(&path).unwrap();
    let tokens: Vec<_> = Lexer::new(&input).collect();

    let actual = tokens
        .iter()
        .map(|t| format!("{t:?}\n"))
        .collect::<String>();

    let snap_path = path.with_extension("").with_extension("snap.txt");
    expect_file![snap_path].assert_eq(&actual);
}
