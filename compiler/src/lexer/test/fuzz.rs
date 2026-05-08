use bumpalo::Bump;

use crate::lexer::Lexer;
use bolero::check;

#[test]
fn lexer() {
    check!().with_type::<String>().for_each(|input: &String| {
        let names = Bump::new();
        let lexer = Lexer::new(input, &names);
        let tokens = lexer.collect::<Vec<_>>();
        #[cfg(not(fuzzing))]
        if tokens.iter().any(Result::is_ok) {
            eprintln!("[len={:3}] {input:?} {tokens:?}", input.len());
        }
        let _ = tokens;
    });
}

#[test]
fn token() {
    check!().with_type::<String>().for_each(|input: &String| {
        let names = Bump::new();
        let token = Lexer::new(input, &names).next();
        #[cfg(not(fuzzing))]
        if let Some(Ok(token)) = token {
            let len = input.len();
            eprintln!("[len={len:03}] {input:?}: {token:?}");
        }
        let _ = token;
    });
}
