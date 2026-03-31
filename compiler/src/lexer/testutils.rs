use crate::lexer::{KEYWORDS, Name, SYMBOLS, Token};

use bolero::generator::{TypeGenerator, ValueGenerator as _, any, one_of, one_value_of};

const IDENTIFIERS: &[&str] = &[
    "x", "y", "z", "foo", "bar", "baz", "add", "mul", "id", "f", "g", "h", "a", "b", "c", "n", "m",
    "p", "q", "r", "x0", "x1", "x2", "x3",
];

pub fn gen_token() -> impl bolero::ValueGenerator<Output = Token<'static>> {
    one_of((
        one_value_of(IDENTIFIERS).map_gen(|s| Token::Ident(Name::new(s))),
        one_value_of(KEYWORDS).map_gen(|(_, t)| t),
        one_value_of(SYMBOLS).map_gen(|(_, t)| t),
        any::<u64>().map_gen(Token::Num),
    ))
}

impl TypeGenerator for Token<'static> {
    fn generate<D: bolero::Driver>(driver: &mut D) -> Option<Self> {
        gen_token().generate(driver)
    }
}
