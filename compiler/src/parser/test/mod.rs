#![allow(
    clippy::get_first,
    clippy::indexing_slicing,
    clippy::use_debug,
    clippy::unwrap_used,
    clippy::wildcard_enum_match_arm,
    reason = "test code"
)]

use std::path::PathBuf;

use expect_test::expect_file;
use rstest::rstest;

use super::*;
use crate::lexer::{Lexer, Token};
use crate::parser::ast::{BinOp, FunName};

fn parse_expr(input: &str) -> String {
    let arena = bumpalo::Bump::new();
    let lexer = Lexer::new(input, &arena);
    let mut parser = Parser::new(lexer, &arena);
    let expr = parser.parse_expr().unwrap();
    format!("{expr:#?}\n")
}

#[rstest]
#[timeout(std::time::Duration::from_secs(if cfg!(miri) { 600 } else { 5 }))]
fn expr(#[files("src/parser/test/expr/*.input.txt")] path: PathBuf) {
    let input = std::fs::read_to_string(&path).unwrap();
    let actual = parse_expr(&input);
    let snap_path = path.with_extension("").with_extension("snap.txt");
    expect_file![snap_path].assert_eq(&actual);
}

#[test]
fn parse_trivial_block() {
    let arena = bumpalo::Bump::new();
    let lexer = Lexer::new("{ 0 + 1 }", &arena);
    let mut parser = Parser::new(lexer, &arena);
    let expr = parser.parse_expr().unwrap();
    match expr {
        Term::Block { .. } => {}
        _ => panic!("expected Block"),
    }
}

#[test]
fn parse_simple_fn() {
    let arena = bumpalo::Bump::new();
    let lexer = Lexer::new("def add(x: u32, y: u32) -> u32 = { x + y };", &arena);
    let mut parser = Parser::new(lexer, &arena);
    let program = parser.parse_program().unwrap();
    assert_eq!(program.defs.len(), 1);
    let f = &program.defs[0];
    assert_eq!(f.name.as_str(), "add");
    assert!(f.phase.is_meta());
    assert_eq!(f.params.map(<[_]>::len), Some(2));
}

#[test]
fn parse_simple_fn_and_junk() {
    let arena = bumpalo::Bump::new();
    let lexer = Lexer::new("def foo() -> u32 = { 0 }; wat", &arena);
    let mut parser = Parser::new(lexer, &arena);
    let program = parser.parse_program();
    assert!(program.is_err());
}

#[test]
fn parse_expr_prec() {
    let arena = bumpalo::Bump::new();
    let lexer = Lexer::new("1 + 2 * 3", &arena);
    let mut parser = Parser::new(lexer, &arena);
    let expr = parser.parse_expr().unwrap();
    match expr {
        Term::App { func, args } => {
            assert_eq!(args.len(), 2);
            assert!(matches!(func, FunName::BinOp(BinOp::Add)));
        }
        _ => panic!("expected App"),
    }
}

#[test]
fn parse_expr_prec2() {
    let arena = bumpalo::Bump::new();
    let lexer = Lexer::new("1 * 2 + 3", &arena);
    let mut parser = Parser::new(lexer, &arena);
    let expr = parser.parse_expr().unwrap();
    match expr {
        Term::App { func, args } => {
            assert_eq!(args.len(), 2);
            assert!(matches!(func, FunName::BinOp(BinOp::Add)));
        }
        _ => panic!("expected App"),
    }
}

#[test]
fn parse_expr_paren() {
    let arena = bumpalo::Bump::new();
    let lexer = Lexer::new("1 * (2 + 3)", &arena);
    let mut parser = Parser::new(lexer, &arena);
    let expr = parser.parse_expr().unwrap();
    match expr {
        Term::App { func, args } => {
            assert_eq!(args.len(), 2);
            assert!(matches!(func, FunName::BinOp(BinOp::Mul)));
        }
        _ => panic!("expected App"),
    }
}

#[test]
fn fuzz_parse_expr() {
    bolero::check!()
        .with_type::<Vec<Token<'static>>>()
        .for_each(|tokens: &Vec<Token<'static>>| {
            let arena = bumpalo::Bump::new();
            let iter = tokens.iter().map(|t| Ok(*t));
            let mut parser = Parser::new(iter, &arena);
            let result = parser.parse_expr();
            if let Ok(expr) = result {
                if parser.next().is_some() {
                    return;
                }
                eprintln!("{tokens:?}: {expr:?}");
            }
        });
}

#[test]
fn fuzz_parse_program() {
    bolero::check!()
        .with_type::<Vec<Token<'static>>>()
        .for_each(|tokens: &Vec<Token<'static>>| {
            let arena = bumpalo::Bump::new();
            let iter = tokens.iter().map(|t| Ok(*t));
            let mut parser = Parser::new(iter, &arena);
            let result = parser.parse_program();
            if let Ok(prog) = result {
                eprintln!("{tokens:?}: {prog:?}");
            }
        });
}
