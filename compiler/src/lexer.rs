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
    DoubleLBracket,
    DoubleRBracket,
    DArrow,
    Num(u64),
    Ident(&'a str),
}

const SYMBOLS: &[(&str, Token<'static>)] = &[
    ("[[", Token::DoubleLBracket),
    ("]]", Token::DoubleRBracket),
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

    fn skip_whitespace(&mut self) {
        self.input = self.input.trim_start();
    }

    fn split_pred<F: Fn(char) -> bool>(&mut self, pred: F) -> &'a str {
        let len = self.input.find(pred).unwrap_or(self.input.len());
        let (token, rest) = self.input.split_at(len);
        self.input = rest;
        token
    }

    fn is_ident_char(c: char) -> bool {
        c.is_ascii_alphanumeric() || c == '_'
    }

    fn read_number(&mut self) -> Result<Token<'a>> {
        let num_str = self.split_pred(|c| !c.is_ascii_digit());
        Ok(Token::Num(num_str.parse()?))
    }

    fn read_ident(&mut self) -> Result<Token<'a>> {
        let ident = self.split_pred(|c| !Self::is_ident_char(c));
        let token = match ident {
            "fn" => Token::Fn,
            "code" => Token::Code,
            "let" => Token::Let,
            "match" => Token::Match,
            _ => Token::Ident(ident),
        };
        Ok(token)
    }

    fn next(&mut self) -> Option<Result<Token<'a>>> {
        self.skip_whitespace();

        // Try matching symbols (longer first due to table order)
        for (pfx, tok) in SYMBOLS {
            if let Some(remainder) = self.input.strip_prefix(pfx) {
                self.input = remainder;
                return Some(Ok(*tok));
            }
        }

        let c = self.input.chars().next()?;

        // Number or identifier
        if c.is_ascii_digit() {
            return Some(self.read_number());
        }
        if c.is_alphabetic() || c == '_' {
            return Some(self.read_ident());
        }

        Some(Err(anyhow!("unexpected character: {}", c)))
    }
}

impl<'a> Iterator for Lexer<'a> {
    type Item = Result<Token<'a>>;

    fn next(&mut self) -> Option<Result<Token<'a>>> {
        self.next()
    }
}
