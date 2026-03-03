use crate::lexer::Lexer;
use bolero::check;

#[test]
fn fuzz_lexer() {
    use std::hint::black_box;

    check!().with_type::<String>().for_each(|input: &String| {
        let lexer = Lexer::new(&input);
        let _ = black_box(lexer.collect::<Vec<_>>());
    });
}
