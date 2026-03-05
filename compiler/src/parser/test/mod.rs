use super::*;
use crate::lexer::Lexer;

#[test]
fn test_parse_trivial_block() {
    let arena = bumpalo::Bump::new();
    let lexer = Lexer::new("{ 0 + 1 }");
    let mut parser = Parser::new(lexer, &arena);
    let expr = parser.parse_expr().unwrap();
    match expr {
        Term::Block { .. } => {}
        _ => panic!("expected Block"),
    }
}

#[test]
fn test_parse_simple_fn() {
    let arena = bumpalo::Bump::new();
    let lexer = Lexer::new("fn add(x: u32, y: u32) -> u32 { x + y }");
    let mut parser = Parser::new(lexer, &arena);
    let program = parser.parse_program().unwrap();
    assert_eq!(program.functions.len(), 1);
    let f = &program.functions[0];
    assert_eq!(f.name.0, "add");
    assert_eq!(f.params.len(), 2);
}

#[test]
fn test_parse_expr_prec() {
    let arena = bumpalo::Bump::new();
    let lexer = Lexer::new("1 + 2 * 3");
    let mut parser = Parser::new(lexer, &arena);
    let expr = parser.parse_expr().unwrap();
    match expr {
        Term::App { func, args } => {
            assert_eq!(args.len(), 2);
            match func {
                Term::Var(name) => assert_eq!(name.0, "+"),
                _ => panic!("expected Var(+)"),
            }
        }
        _ => panic!("expected App"),
    }
}

#[test]
fn test_parse_expr_prec2() {
    let arena = bumpalo::Bump::new();
    let lexer = Lexer::new("1 * 2 + 3");
    let mut parser = Parser::new(lexer, &arena);
    let expr = parser.parse_expr().unwrap();
    match expr {
        Term::App { func, args } => {
            assert_eq!(args.len(), 2);
            match func {
                Term::Var(name) => assert_eq!(name.0, "+"),
                _ => panic!("expected Var(+)"),
            }
        }
        _ => panic!("expected App"),
    }
}

#[test]
fn test_parse_expr_paren() {
    let arena = bumpalo::Bump::new();
    let lexer = Lexer::new("1 * (2 + 3)");
    let mut parser = Parser::new(lexer, &arena);
    let expr = parser.parse_expr().unwrap();
    match expr {
        Term::App { func, args } => {
            assert_eq!(args.len(), 2);
            match func {
                Term::Var(name) => assert_eq!(name.0, "*"),
                _ => panic!("expected Var(*)"),
            }
        }
        _ => panic!("expected App"),
    }
}
