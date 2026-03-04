use std::iter::Peekable;

use anyhow::{Context, Result};

use crate::ast::{Function, Let, MatchArm, Name, Param, Pat, Phase, Program, Term};
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
        Self {
            tokens: tokens.peekable(),
            arena,
        }
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

    fn take_ident(&mut self) -> Result<&'a str> {
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
        let functions = self.arena.alloc(functions.into_boxed_slice());
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
        let body = self.arena.alloc(body);

        Ok(Function {
            phase,
            name: Name(name),
            params: self.arena.alloc(params.into_boxed_slice()),
            ret_ty,
            body,
        })
    }

    fn parse_params(&mut self) -> Result<Vec<Param<'a>>> {
        let mut params = Vec::new();
        if self.peek() == Some(Token::RParen) {
            return Ok(params);
        }
        loop {
            let name = Name(self.take_ident().context("expected parameter name")?);
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
        Ok(params)
    }

    fn parse_block(&mut self) -> Result<Term<'a>> {
        self.take(Token::LBrace).context("expected '{'")?;
        let mut stmts = Vec::new();

        while self.peek() == Some(Token::Let) {
            let let_stmt = self.parse_let_stmt().context("parsing let statement")?;
            stmts.push(let_stmt);
        }

        let expr = self.parse_expr().context("parsing expression in block")?;
        self.take(Token::RBrace).context("expected '}'")?;

        Ok(Term::Block {
            stmts: self.arena.alloc(stmts.into_boxed_slice()),
            expr: self.arena.alloc(expr),
        })
    }

    fn parse_let_stmt(&mut self) -> Result<Let<'a>> {
        self.take(Token::Let).context("expected 'let'")?;
        let name = self.take_ident().context("expected variable name")?;
        let ty_opt = if self.peek() == Some(Token::Colon) {
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
        let ty = ty_opt.map(|t| &*self.arena.alloc(t));
        let expr = &*self.arena.alloc(expr);
        Ok(Let {
            name: Name(name),
            ty,
            expr,
        })
    }

    fn parse_expr(&mut self) -> Result<Term<'a>> {
        self.parse_expr_prec(1)
    }

    fn parse_expr_prec(&mut self, min_prec: u8) -> Result<Term<'a>> {
        let mut lhs = self.parse_atom().context("parsing atom")?;

        loop {
            let Some(op) = Self::binop_prec(self.peek()) else {
                break;
            };
            if op.prec < min_prec {
                break;
            }
            let next_min_prec = if op.assoc == Assoc::Left {
                op.prec + 1
            } else {
                op.prec
            };
            self.next();

            let rhs = self
                .parse_expr_prec(next_min_prec)
                .context("parsing right-hand side of binary expression")?;

            let func = self.arena.alloc(Term::Var(Name(op.name)));
            let lhs_alloc = self.arena.alloc(lhs);
            let rhs_alloc = self.arena.alloc(rhs);
            let args = self.arena.alloc_slice_fill_iter([&*lhs_alloc, &*rhs_alloc]);
            lhs = Term::App { func, args };
        }

        if self.peek() == Some(Token::Bang) {
            self.next();
            let expr = self.parse_expr_prec(7).context("parsing operand of '!'")?;
            let func = self.arena.alloc(Term::Var(Name("!")));
            let expr_alloc = self.arena.alloc(expr);
            let args = self.arena.alloc_slice_fill_iter([&*expr_alloc]);
            lhs = Term::App { func, args };
        }

        Ok(lhs)
    }

    fn binop_prec(token: Option<Token<'a>>) -> Option<BinOp> {
        match token {
            Some(Token::Bar) => Some(BinOp::new("|", 1, Assoc::Left)),
            Some(Token::Ampersand) => Some(BinOp::new("&", 2, Assoc::Left)),
            Some(Token::EqEq) => Some(BinOp::new("==", 3, Assoc::Left)),
            Some(Token::Ne) => Some(BinOp::new("!=", 3, Assoc::Left)),
            Some(Token::Lt) => Some(BinOp::new("<", 3, Assoc::Left)),
            Some(Token::Gt) => Some(BinOp::new(">", 3, Assoc::Left)),
            Some(Token::Le) => Some(BinOp::new("<=", 3, Assoc::Left)),
            Some(Token::Ge) => Some(BinOp::new(">=", 3, Assoc::Left)),
            Some(Token::Plus) => Some(BinOp::new("+", 4, Assoc::Left)),
            Some(Token::Minus) => Some(BinOp::new("-", 4, Assoc::Left)),
            Some(Token::Star) => Some(BinOp::new("*", 5, Assoc::Left)),
            Some(Token::Slash) => Some(BinOp::new("/", 5, Assoc::Left)),
            _ => None,
        }
    }

    fn parse_atom(&mut self) -> Result<Term<'a>> {
        let token = self.next().context("expected expression")??;
        match token {
            Token::Num(n) => Ok(Term::Lit(n)),
            Token::Ident(name) => Ok(Term::Var(Name(name))),
            Token::LParen => {
                let expr = self
                    .parse_expr()
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
                let mut stmts = Vec::new();
                loop {
                    match self.peek() {
                        Some(Token::Let) => {
                            let let_stmt = self
                                .parse_let_stmt()
                                .context("parsing let in block quote")?;
                            stmts.push(let_stmt);
                        }
                        Some(Token::RBrace) => {
                            self.next();
                            let expr = self
                                .parse_expr()
                                .context("parsing expression in block quote")?;
                            let block = Term::Block {
                                stmts: self.arena.alloc(stmts.into_boxed_slice()),
                                expr: self.arena.alloc(expr),
                            };
                            return Ok(Term::Quote(self.arena.alloc(block)));
                        }
                        Some(_) => {
                            let expr = self
                                .parse_expr()
                                .context("parsing expression in block quote")?;
                            if self.peek() == Some(Token::Semi) {
                                self.next();
                            }
                            stmts.push(Let {
                                name: Name("_"),
                                ty: None,
                                expr: self.arena.alloc(expr),
                            });
                        }
                        None => return Err(anyhow::anyhow!("unclosed block quote")),
                    }
                }
            }
            Token::DollarLParen => {
                let expr = self.parse_expr().context("parsing spliced expression")?;
                self.take(Token::RParen)
                    .context("expected ')' after splice")?;
                Ok(Term::Splice(self.arena.alloc(expr)))
            }
            Token::DollarLBrace => {
                let mut stmts = Vec::new();
                loop {
                    match self.peek() {
                        Some(Token::Let) => {
                            let let_stmt = self
                                .parse_let_stmt()
                                .context("parsing let in block splice")?;
                            stmts.push(let_stmt);
                        }
                        Some(Token::RBrace) => {
                            self.next();
                            let expr = self
                                .parse_expr()
                                .context("parsing expression in block splice")?;
                            let block = Term::Block {
                                stmts: self.arena.alloc(stmts.into_boxed_slice()),
                                expr: self.arena.alloc(expr),
                            };
                            return Ok(Term::Splice(self.arena.alloc(block)));
                        }
                        Some(_) => {
                            let expr = self
                                .parse_expr()
                                .context("parsing expression in block splice")?;
                            if self.peek() == Some(Token::Semi) {
                                self.next();
                            }
                            stmts.push(Let {
                                name: Name("_"),
                                ty: None,
                                expr: self.arena.alloc(expr),
                            });
                        }
                        None => return Err(anyhow::anyhow!("unclosed block splice")),
                    }
                }
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
                Ok(Term::Match {
                    scrutinee,
                    arms: self.arena.alloc(arms.into_boxed_slice()),
                })
            }
            Token::LBrace => {
                let mut stmts = Vec::new();

                while self.peek() == Some(Token::Let) {
                    let let_stmt = self.parse_let_stmt().context("parsing let in block")?;
                    stmts.push(let_stmt);
                }

                let expr = self.parse_expr().context("parsing expression in block")?;
                self.take(Token::RBrace)
                    .context("expected '}' after expression in block")?;

                Ok(Term::Block {
                    stmts: self.arena.alloc(stmts.into_boxed_slice()),
                    expr: self.arena.alloc(expr),
                })
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
            arms.push(MatchArm {
                pat: self.arena.alloc(pat),
                body,
            });
        }
        Ok(arms)
    }

    fn parse_pattern(&mut self) -> Result<Pat<'a>> {
        let token = self.next().context("expected pattern")??;
        match token {
            Token::Num(n) => Ok(Pat::Lit(n)),
            Token::Ident(name) => Ok(Pat::Name(Name(name))),
            _ => Err(anyhow::anyhow!("unexpected token in pattern: {:?}", token)),
        }
    }
}

struct BinOp {
    name: &'static str,
    prec: u8,
    assoc: Assoc,
}

#[derive(PartialEq)]
enum Assoc {
    Left,
    #[expect(dead_code)]
    Right,
}

impl BinOp {
    fn new(name: &'static str, prec: u8, assoc: Assoc) -> Self {
        Self { name, prec, assoc }
    }
}

#[cfg(test)]
mod test;
