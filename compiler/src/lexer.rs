use anyhow::{anyhow, Result};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Token<'a> {
    Fn,
    Code,
    Let,
    Match,
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Arrow,
    Colon,
    Eq,
    Comma,
    Semi,
    Bar,
    Ampersand,
    EqEq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    Plus,
    Minus,
    Star,
    Slash,
    Bang,
    HashLParen,
    HashLBrace,
    DollarLParen,
    DollarLBrace,
    LBracketLBracket,
    RBracketRBracket,
    DArrow,
    Num(u64),
    Ident(&'a str),
}

const SYMBOLS: &[(&str, Token<'static>)] = &[
    ("[[", Token::LBracketLBracket),
    ("]]", Token::RBracketRBracket),
    ("#(", Token::HashLParen),
    ("#{", Token::HashLBrace),
    ("$(", Token::DollarLParen),
    ("${", Token::DollarLBrace),
    ("=>", Token::DArrow),
    (">=", Token::Ge),
    ("<=", Token::Le),
    ("==", Token::EqEq),
    ("!=", Token::Ne),
    ("->", Token::Arrow),
    ("(", Token::LParen),
    (")", Token::RParen),
    ("[", Token::LBracket),
    ("]", Token::RBracket),
    ("{", Token::LBrace),
    ("}", Token::RBrace),
    (":", Token::Colon),
    ("=", Token::Eq),
    (",", Token::Comma),
    (";", Token::Semi),
    ("|", Token::Bar),
    ("&", Token::Ampersand),
    ("<", Token::Lt),
    (">", Token::Gt),
    ("+", Token::Plus),
    ("-", Token::Minus),
    ("*", Token::Star),
    ("/", Token::Slash),
    ("!", Token::Bang),
];

pub struct Lexer<'a> {
    input: &'a str,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        Self { input }
    }

    fn consume(&mut self) -> Option<char> {
        let c = self.input.chars().next()?;
        self.input = self.input.strip_prefix(c).unwrap();
        Some(c)
    }

    fn skip_whitespace(&mut self) {
        self.input = self.input.trim_start();
    }

    fn read_number(&mut self, first: char) -> Result<Token<'a>> {
        let start = self.input.len();
        let len = self
            .input
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .count();
        self.input = &self.input[len..];
        let num_str = &self.input[start - first.len_utf8()..start];
        let num = num_str
            .parse()
            .map_err(|_| anyhow!("invalid number: {}", num_str))?;
        Ok(Token::Num(num))
    }

    fn read_ident(&mut self, first: char) -> Result<Token<'a>> {
        let start = self.input.len();
        let len = self
            .input
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .count();
        self.input = &self.input[len..];
        let ident = &self.input[start - first.len_utf8()..start];
        let token = match ident {
            "fn" => Token::Fn,
            "code" => Token::Code,
            "let" => Token::Let,
            "match" => Token::Match,
            _ => Token::Ident(ident),
        };
        Ok(token)
    }
}

impl<'a> Iterator for Lexer<'a> {
    type Item = Result<Token<'a>>;

    fn next(&mut self) -> Option<Result<Token<'a>>> {
        self.skip_whitespace();

        let c = self.consume()?;

        // Try matching symbols (longer first due to table order)
        for (pfx, tok) in SYMBOLS {
            if let Some(remainder) = self.input.strip_prefix(pfx) {
                self.input = remainder;
                return Some(Ok(*tok));
            }
        }

        // Number or identifier
        if c.is_ascii_digit() {
            return Some(self.read_number(c));
        }
        if c.is_alphabetic() || c == '_' {
            return Some(self.read_ident(c));
        }

        Some(Err(anyhow!("unexpected character: {}", c)))
    }
}
