use std::iter::Peekable;

use anyhow::{Context, Result};

use crate::ast::{BinOp, FunName, Function, Let, MatchArm, Name, UnOp, Param, Pat, Phase, Program, Term};
use crate::lexer::Token;

pub struct Parser<'a, I>
where
    I: Iterator<Item = Result<Token<'a>>>,
{
    tokens: Peekable<I>,
    arena: &'a bumpalo::Bump,
}

impl<'a, I> Parser<'a, I>
where
    I: Iterator<Item = Result<Token<'a>>>,
{
    pub fn new(tokens: I, arena: &'a bumpalo::Bump) -> Self {
        let tokens = tokens.peekable();
        Self { tokens, arena }
    }

    fn peek(&mut self) -> Option<Token<'a>> {
        self.tokens.peek().and_then(|r| r.as_ref().ok().copied())
    }

    fn next(&mut self) -> Option<Result<Token<'a>>> {
        self.tokens.next()
    }

    fn take(&mut self, expected: Token<'a>) -> Result<Token<'a>> {
        match self.next() {
            Some(Ok(token)) if token == expected => Ok(token),
            Some(Ok(token)) => Err(anyhow::anyhow!("expected {expected:?}, got {token:?}")),
            Some(Err(e)) => Err(e),
            None => Err(anyhow::anyhow!("expected {expected:?}, got end of input")),
        }
    }

    fn take_ident(&mut self) -> Result<Name<'a>> {
        match self.next() {
            Some(Ok(Token::Ident(name))) => Ok(name),
            Some(Ok(token)) => Err(anyhow::anyhow!("expected identifier, got {token:?}")),
            Some(Err(e)) => Err(e),
            None => Err(anyhow::anyhow!("expected identifier, got end of input")),
        }
    }

    pub fn parse_program(&mut self) -> Result<Program<'a>> {
        let mut functions = Vec::new();
        while self.peek().is_some() {
            let fun = self.parse_fn_def().context("parsing function definition")?;
            functions.push(fun);
        }
        let functions = self.arena.alloc_slice_fill_iter(functions);
        Ok(Program { functions })
    }

    fn parse_fn_def(&mut self) -> Result<Function<'a>> {
        let phase = if self.peek() == Some(Token::Code) {
            self.next();
            Phase::Object
        } else {
            Phase::Meta
        };

        self.take(Token::Fn).context("expected 'fn'")?;
        let name = self.take_ident().context("expected function name")?;

        self.take(Token::LParen).context("expected '('")?;
        let params = self.parse_params()?;
        self.take(Token::RParen).context("expected ')'")?;

        self.take(Token::Arrow).context("expected '->'")?;

        let ret_ty = self
            .parse_expr()
            .context("expected return type expression")?;

        let body = self.parse_block().context("expected function body")?;

        let ret_ty = self.arena.alloc(ret_ty);

        Ok(Function {
            phase,
            name,
            params,
            ret_ty,
            body,
        })
    }

    fn parse_params(&mut self) -> Result<&'a [Param<'a>]> {
        let mut params = Vec::new();
        if self.peek() == Some(Token::RParen) {
            return Ok(self.arena.alloc_slice_fill_iter(params));
        }
        loop {
            let name = self.take_ident().context("expected parameter name")?;
            self.take(Token::Colon)
                .context("expected ':' in parameter")?;
            let ty = self.parse_expr().context("expected parameter type")?;
            let ty = self.arena.alloc(ty);
            params.push(Param { name, ty });
            if self.peek() == Some(Token::Comma) {
                self.next();
            } else {
                break;
            }
        }
        Ok(self.arena.alloc_slice_fill_iter(params))
    }

    fn parse_block(&mut self) -> Result<&'a Term<'a>> {
        self.take(Token::LBrace).context("expected '{'")?;
        let (stmts, expr) = self.parse_block_inner()?;
        Ok(self.arena.alloc(Term::Block { stmts, expr }))
    }

    fn parse_block_inner(&mut self) -> Result<(&'a [Let<'a>], &'a Term<'a>)> {
        let mut stmts = Vec::new();

        while self.peek() == Some(Token::Let) {
            let let_stmt = self.parse_let_stmt().context("parsing let statement")?;
            stmts.push(let_stmt);
        }

        let expr = self.parse_expr().context("parsing expression in block")?;
        self.take(Token::RBrace).context("expected '}'")?;

        let stmts = self.arena.alloc_slice_fill_iter(stmts);
        Ok((stmts, self.arena.alloc(expr)))
    }

    fn parse_let_stmt(&mut self) -> Result<Let<'a>> {
        self.take(Token::Let).context("expected 'let'")?;
        let name = self.take_ident().context("expected variable name")?;
        let ty = if self.peek() == Some(Token::Colon) {
            self.next();
            Some(self.parse_expr().context("expected type in let binding")?)
        } else {
            None
        };
        self.take(Token::Eq)
            .context("expected '=' in let binding")?;
        let expr = self
            .parse_expr()
            .context("expected expression in let binding")?;
        self.take(Token::Semi)
            .context("expected ';' after let binding")?;
        let expr = self.arena.alloc(expr);
        Ok(Let { name, ty, expr })
    }

    fn parse_expr(&mut self) -> Result<&'a Term<'a>> {
        Ok(self.arena.alloc(self.parse_expr_owned()?))
    }

    fn parse_expr_owned(&mut self) -> Result<Term<'a>> {
        self.parse_expr_prec(1)
    }

    fn parse_expr_prec(&mut self, min_prec: u8) -> Result<Term<'a>> {
        let mut lhs = if self.peek() == Some(Token::Bang) {
            self.next();
            let expr = self
                .parse_expr_prec(Self::NOT_PREC)
                .context("parsing operand of '!'")?;
            let expr = &*self.arena.alloc(expr);
            Term::App {
                func: FunName::UnOp(UnOp::Not),
                args: self.arena.alloc_slice_fill_iter([expr]),
            }
        } else {
            self.parse_atom_owned()?
        };

        loop {
            let Some(op) = Self::binop_prec(self.peek()) else {
                break;
            };
            let (prec, assoc) = Self::binop_info(op);
            if prec < min_prec {
                break;
            }
            let next_min_prec = match assoc {
                Assoc::Left => prec + 1,
                Assoc::Right => prec,
            };
            self.next();

            let rhs = self
                .parse_expr_prec(next_min_prec)
                .context("parsing right-hand side of binary expression")?;
            let rhs = &*self.arena.alloc(rhs);

            let func = FunName::BinOp(op);
            let lhs_ref = &*self.arena.alloc(lhs);
            let args = self.arena.alloc_slice_fill_iter([lhs_ref, rhs]);
            lhs = Term::App { func, args };
        }

        Ok(lhs)
    }

    const NOT_PREC: u8 = 7;

    fn binop_prec(token: Option<Token<'a>>) -> Option<BinOp> {
        match token? {
            Token::Bar => Some(BinOp::BitOr),
            Token::Ampersand => Some(BinOp::BitAnd),
            Token::EqEq => Some(BinOp::Eq),
            Token::Ne => Some(BinOp::Ne),
            Token::Lt => Some(BinOp::Lt),
            Token::Gt => Some(BinOp::Gt),
            Token::Le => Some(BinOp::Le),
            Token::Ge => Some(BinOp::Ge),
            Token::Plus => Some(BinOp::Add),
            Token::Minus => Some(BinOp::Sub),
            Token::Star => Some(BinOp::Mul),
            Token::Slash => Some(BinOp::Div),
            _ => None,
        }
    }

    fn binop_info(op: BinOp) -> (u8, Assoc) {
        match op {
            BinOp::BitOr => (1, Assoc::Left),
            BinOp::BitAnd => (2, Assoc::Left),
            BinOp::Eq => (3, Assoc::Left),
            BinOp::Ne => (3, Assoc::Left),
            BinOp::Lt => (3, Assoc::Left),
            BinOp::Gt => (3, Assoc::Left),
            BinOp::Le => (3, Assoc::Left),
            BinOp::Ge => (3, Assoc::Left),
            BinOp::Add => (4, Assoc::Left),
            BinOp::Sub => (4, Assoc::Left),
            BinOp::Mul => (5, Assoc::Left),
            BinOp::Div => (5, Assoc::Left),
        }
    }

    fn parse_atom_owned(&mut self) -> Result<Term<'a>> {
        let token = self.next().context("expected expression")??;
        match token {
            Token::Num(n) => Ok(Term::Lit(n)),
            Token::Ident(name) => {
                if self.peek() == Some(Token::LParen) {
                    self.next();
                    let mut args = Vec::new();
                    while self.peek() != Some(Token::RParen) {
                        let arg = self.parse_expr().context("parsing function argument")?;
                        args.push(arg);
                        if self.peek() == Some(Token::Comma) {
                            self.next();
                        } else {
                            break;
                        }
                    }
                    self.take(Token::RParen)
                        .context("expected ')' after function arguments")?;
                    let args = self.arena.alloc_slice_fill_iter(args);
                    Ok(Term::App {
                        func: FunName::Name(name),
                        args,
                    })
                } else {
                    Ok(Term::Var(name))
                }
            }
            Token::LParen => {
                let expr = self
                    .parse_expr_owned()
                    .context("parsing expression in parentheses")?;
                self.take(Token::RParen)
                    .context("expected ')' after parenthesized expression")?;
                Ok(expr)
            }
            Token::HashLParen => {
                let expr = self.parse_expr().context("parsing quoted expression")?;
                self.take(Token::RParen)
                    .context("expected ')' after quotation")?;
                Ok(Term::Quote(self.arena.alloc(expr)))
            }
            Token::HashLBrace => {
                let (stmts, expr) = self.parse_block_inner()?;
                Ok(Term::Quote(self.arena.alloc(Term::Block { stmts, expr })))
            }
            Token::DollarLParen => {
                let expr = self.parse_expr().context("parsing spliced expression")?;
                self.take(Token::RParen)
                    .context("expected ')' after splice")?;
                Ok(Term::Splice(self.arena.alloc(expr)))
            }
            Token::DollarLBrace => {
                let (stmts, expr) = self.parse_block_inner()?;
                Ok(Term::Splice(self.arena.alloc(Term::Block { stmts, expr })))
            }
            Token::DoubleLBracket => {
                let expr = self.parse_expr().context("parsing lifted expression")?;
                self.take(Token::DoubleRBracket)
                    .context("expected ']]' after lifted expression")?;
                Ok(Term::Lift(self.arena.alloc(expr)))
            }
            Token::Match => {
                let scrutinee = self.parse_expr().context("parsing match scrutinee")?;
                self.take(Token::LBrace)
                    .context("expected '{' after match expression")?;
                let arms = self.parse_match_arms()?;
                self.take(Token::RBrace)
                    .context("expected '}' after match arms")?;
                let scrutinee = self.arena.alloc(scrutinee);
                let arms = self.arena.alloc_slice_fill_iter(arms);
                Ok(Term::Match { scrutinee, arms })
            }
            Token::LBrace => {
                let (stmts, expr) = self.parse_block_inner()?;
                Ok(Term::Block { stmts, expr })
            }
            _ => Err(anyhow::anyhow!(
                "unexpected token in expression: {:?}",
                token
            )),
        }
    }

    fn parse_match_arms(&mut self) -> Result<Vec<MatchArm<'a>>> {
        let mut arms = Vec::new();
        while self.peek().is_some() && self.peek() != Some(Token::RBrace) {
            let pat = self.parse_pattern().context("parsing match pattern")?;
            self.take(Token::DArrow)
                .context("expected '=>' in match arm")?;
            let body = self.parse_expr().context("parsing match arm body")?;
            self.take(Token::Comma)
                .context("expected ',' after match arm")?;
            let body = self.arena.alloc(body);
            arms.push(MatchArm { pat, body });
        }
        Ok(arms)
    }

    fn parse_pattern(&mut self) -> Result<Pat<'a>> {
        let token = self.next().context("expected pattern")??;
        match token {
            Token::Num(n) => Ok(Pat::Lit(n)),
            Token::Ident(name) => Ok(Pat::Name(name)),
            _ => Err(anyhow::anyhow!("unexpected token in pattern: {:?}", token)),
        }
    }
}

#[derive(PartialEq)]
enum Assoc {
    Left,
    Right,
}

#[cfg(test)]
mod test;
