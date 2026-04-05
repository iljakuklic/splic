use anyhow::{Result, anyhow};

pub use crate::common::Name;
pub use token::Token;

#[cfg(test)]
pub mod testutils;
mod token;

pub struct Lexer<'a> {
    input: &'a str,
}

impl<'a> Lexer<'a> {
    pub const fn new(input: &'a str) -> Self {
        Self { input }
    }

    #[inline]
    fn skip_whitespace(&mut self) {
        loop {
            self.input = self.input.trim_ascii_start();
            if self.input.starts_with("//") {
                match self.input.split_once('\n') {
                    Some((_comment, rest)) => {
                        self.input = rest;
                        continue;
                    }
                    None => self.input = "",
                }
            }
            break;
        }
    }

    fn split_pred<F: Fn(char) -> bool>(&mut self, pred: F) -> &'a str {
        let len = self.input.find(pred).unwrap_or(self.input.len());
        let (token, rest) = self.input.split_at(len);
        self.input = rest;
        token
    }

    const fn is_ident_char(c: char) -> bool {
        c.is_ascii_alphanumeric() || c == '_'
    }

    fn read_number(&mut self) -> Result<Token<'a>> {
        let num_str = self.split_pred(|c| !c.is_ascii_digit());
        Ok(Token::Num(num_str.parse()?))
    }

    fn read_ident(&mut self) -> Token<'a> {
        let ident = self.split_pred(|c| !Self::is_ident_char(c));
        Token::KEYWORDS
            .iter()
            .find(|(kw, _)| *kw == ident)
            .map_or(Token::Ident(Name::new(ident)), |(_, tok)| *tok)
    }

    #[inline]
    fn read_token_impl(&mut self) -> Option<Result<Token<'a>>> {
        let c = self.input.chars().next()?;

        // Try matching symbols (longer first due to table order)
        for (pfx, tok) in Token::SYMBOLS {
            if let Some(remainder) = self.input.strip_prefix(pfx) {
                self.input = remainder;
                return Some(Ok(*tok));
            }
        }

        // Number or identifier
        if c.is_ascii_digit() {
            return Some(self.read_number());
        }
        if Self::is_ident_char(c) {
            return Some(Ok(self.read_ident()));
        }

        // Unknown character - consume it to avoid infinite loop
        self.input = &self.input[c.len_utf8()..];
        Some(Err(anyhow!("unexpected character: {c}")))
    }

    #[inline]
    fn read_token(&mut self) -> Option<Result<Token<'a>>> {
        let orig_len = self.input.len();
        let result = self.read_token_impl();
        assert!(
            result.is_none() || self.input.len() < orig_len,
            "lexer made no progress and did not return None (infinite loop guard)"
        );
        result
    }

    #[inline]
    fn next(&mut self) -> Option<Result<Token<'a>>> {
        self.skip_whitespace();
        self.read_token()
    }
}

impl<'a> Iterator for Lexer<'a> {
    type Item = Result<Token<'a>>;

    fn next(&mut self) -> Option<Result<Token<'a>>> {
        self.next()
    }
}

#[cfg(test)]
mod test;
