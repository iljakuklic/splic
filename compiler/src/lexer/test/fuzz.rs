use crate::lexer::Lexer;
use bolero::check;

#[test]
fn lexer() {
    check!().with_type::<String>().for_each(|input: &String| {
        let lexer = Lexer::new(input);
        let tokens = lexer.collect::<Vec<_>>();
        if tokens.iter().any(Result::is_ok) {
            eprintln!("[len={:3}] {input:?} {tokens:?}", input.len());
        }
    });
}

#[test]
fn token() {
    check!().with_type::<String>().for_each(|input: &String| {
        let token = Lexer::new(input).next();
        if let Some(Ok(token)) = token {
            let len = input.len();
            eprintln!("[len={len:03}] {input:?}: {token:?}");
        }
    });
}
