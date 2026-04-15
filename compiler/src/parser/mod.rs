use std::iter::Peekable;

use anyhow::{Context as _, Result};

use crate::common::Precedence;
use crate::lexer::Token;
use crate::parser::ast::{
    Assoc, BinOp, FunName, Function, Let, MatchArm, Name, Param, Pat, Phase, Program, Term, UnOp,
};

pub mod ast;

pub struct Parser<'names, 'ast, I>
where
    I: Iterator<Item = Result<Token<'names>>>,
{
    tokens: Peekable<I>,
    arena: &'ast bumpalo::Bump,
}

impl<'names, 'ast, I> Parser<'names, 'ast, I>
where
    I: Iterator<Item = Result<Token<'names>>>,
{
    pub fn new(tokens: I, arena: &'ast bumpalo::Bump) -> Self {
        let tokens = tokens.peekable();
        Self { tokens, arena }
    }

    fn peek(&mut self) -> Option<Token<'names>> {
        self.tokens.peek().and_then(|r| r.as_ref().ok().copied())
    }

    fn next(&mut self) -> Option<Result<Token<'names>>> {
        self.tokens.next()
    }

    /// Helper for extracting a token with a custom validation closure
    fn expect_token<T, F>(&mut self, description: &str, f: F) -> Result<T>
    where
        F: FnOnce(Token<'names>) -> Option<T>,
    {
        match self.next() {
            Some(Ok(token)) => {
                f(token).ok_or_else(|| anyhow::anyhow!("expected {description}, got {token:?}"))
            }
            Some(Err(e)) => Err(e),
            None => Err(anyhow::anyhow!("expected {description}, got end of input")),
        }
    }

    fn take(&mut self, expected: Token<'names>) -> Result<Token<'names>> {
        self.expect_token(&format!("{expected:?}"), |token| {
            (token == expected).then_some(token)
        })
    }

    fn take_ident(&mut self) -> Result<&'names Name> {
        self.expect_token("identifier", |token| {
            if let Token::Ident(name) = token {
                Some(name)
            } else {
                None
            }
        })
    }

    /// Consume a token if it matches the expected token
    fn consume_if(&mut self, token: Token<'names>) -> bool {
        if matches!(self.peek(), Some(t) if t == token) {
            self.next();
            true
        } else {
            false
        }
    }

    /// Allocate a term and return a reference to it
    fn alloc(&self, term: Term<'names, 'ast>) -> &'ast Term<'names, 'ast> {
        &*self.arena.alloc(term)
    }

    /// Parse a quoted expression: #(...)
    fn parse_quoted_expr(&mut self) -> Result<Term<'names, 'ast>> {
        let expr = self.parse_expr().context("parsing quoted expression")?;
        self.take(Token::RParen)
            .context("expected ')' after quotation")?;
        Ok(Term::Quote(expr))
    }

    /// Parse a quoted block: #{...}
    fn parse_quoted_block(&mut self) -> Result<Term<'names, 'ast>> {
        let (stmts, expr) = self.parse_block_inner()?;
        Ok(Term::Quote(self.alloc(Term::Block { stmts, expr })))
    }

    /// Parse a spliced expression: $(...)
    fn parse_spliced_expr(&mut self) -> Result<Term<'names, 'ast>> {
        let expr = self.parse_expr().context("parsing spliced expression")?;
        self.take(Token::RParen)
            .context("expected ')' after splice")?;
        Ok(Term::Splice(expr))
    }

    /// Parse a spliced block: ${...}
    fn parse_spliced_block(&mut self) -> Result<Term<'names, 'ast>> {
        let (stmts, expr) = self.parse_block_inner()?;
        Ok(Term::Splice(self.alloc(Term::Block { stmts, expr })))
    }

    /// Parse a lifted expression: [[...]]
    fn parse_lifted_expr(&mut self) -> Result<Term<'names, 'ast>> {
        let expr = self.parse_expr().context("parsing lifted expression")?;
        self.take(Token::DoubleRBracket)
            .context("expected ']]' after lifted expression")?;
        Ok(Term::Lift(expr))
    }

    /// Parse a comma-separated list of items bounded by a terminator token
    fn parse_separated_list<T, F>(
        &mut self,
        terminator: Token<'names>,
        mut parser: F,
    ) -> Result<Vec<T>>
    where
        F: FnMut(&mut Self) -> Result<T>,
    {
        let mut items = Vec::new();
        if matches!(self.peek(), Some(t) if t == terminator) {
            return Ok(items);
        }
        loop {
            items.push(parser(self)?);
            if !self.consume_if(Token::Comma) {
                break;
            }
        }
        Ok(items)
    }

    pub fn parse_program(&mut self) -> Result<Program<'names, 'ast>> {
        let mut functions = Vec::new();
        while self.peek().is_some() {
            let fun = self.parse_fn_def()?;
            functions.push(fun);
        }
        let functions = self.arena.alloc_slice_fill_iter(functions);
        Ok(Program { functions })
    }

    fn parse_fn_def(&mut self) -> Result<Function<'names, 'ast>> {
        let phase = if self.consume_if(Token::Code) {
            Phase::Object
        } else {
            Phase::Meta
        };

        self.take(Token::Def).context("expected 'def'")?;
        let name = self.take_ident().context("expected function name")?;

        self.parse_fn_def_after_name(phase, name)
            .with_context(|| format!("in function `{name}`"))
    }

    fn parse_fn_def_after_name(
        &mut self,
        phase: Phase,
        name: &'names Name,
    ) -> Result<Function<'names, 'ast>> {
        self.take(Token::LParen).context("expected '('")?;
        let params = self.parse_params()?;
        self.take(Token::RParen).context("expected ')'")?;

        self.take(Token::Arrow).context("expected '->'")?;

        let ret_ty = self
            .parse_expr()
            .context("expected return type expression")?;

        self.take(Token::Eq).context("expected '='")?;
        let body = self.parse_expr().context("expected function body")?;
        self.take(Token::Semi)
            .context("expected ';' after function body")?;

        Ok(Function {
            phase,
            name,
            params,
            ret_ty,
            body,
        })
    }

    fn parse_params(&mut self) -> Result<&'ast [Param<'names, 'ast>]> {
        let params = self.parse_separated_list(Token::RParen, |parser| {
            let name = parser.take_ident().context("expected parameter name")?;
            parser
                .take(Token::Colon)
                .context("expected ':' in parameter")?;
            let ty = parser.parse_expr().context("expected parameter type")?;
            let ty = parser.arena.alloc(ty);
            Ok(Param { name, ty })
        })?;
        Ok(self.arena.alloc_slice_fill_iter(params))
    }

    fn parse_block_inner(
        &mut self,
    ) -> Result<(&'ast [Let<'names, 'ast>], &'ast Term<'names, 'ast>)> {
        let mut stmts = Vec::new();

        while self.peek() == Some(Token::Let) {
            let let_stmt = self.parse_let_stmt().context("parsing let statement")?;
            stmts.push(let_stmt);
        }

        let expr = self.parse_expr().context("parsing expression in block")?;
        self.take(Token::RBrace).context("expected '}'")?;

        let stmts = self.arena.alloc_slice_fill_iter(stmts);
        Ok((stmts, expr))
    }

    fn parse_let_stmt(&mut self) -> Result<Let<'names, 'ast>> {
        self.take(Token::Let).context("expected 'let'")?;
        let name = self.take_ident().context("expected variable name")?;
        let ty = if self.consume_if(Token::Colon) {
            Some(self.parse_expr().context("expected type in let binding")?)
        } else {
            None
        };
        self.take(Token::Eq)
            .context("expected '=' in let binding")?;
        let expr = self
            .parse_expr()
            .with_context(|| format!("in let binding `{name}`"))?;
        self.take(Token::Semi)
            .context("expected ';' after let binding")?;
        Ok(Let { name, ty, expr })
    }

    fn parse_expr(&mut self) -> Result<&'ast Term<'names, 'ast>> {
        Ok(self.arena.alloc(self.parse_expr_owned()?))
    }

    fn parse_expr_owned(&mut self) -> Result<Term<'names, 'ast>> {
        self.parse_expr_prec(Precedence::MIN)
    }

    fn parse_expr_prec(&mut self, min_prec: Precedence) -> Result<Term<'names, 'ast>> {
        let mut lhs = if let Some(op) = self.match_unop() {
            self.next();
            let expr = self
                .parse_expr_prec(op.precedence())
                .context("parsing operand of '!'")?;
            let expr = self.alloc(expr);
            Term::App {
                func: FunName::UnOp(op),
                args: self.arena.alloc_slice_fill_iter([expr]),
            }
        } else {
            self.parse_atom_owned()?
        };

        loop {
            if self.peek() == Some(Token::LParen) {
                self.next();
                let args = self.parse_separated_list(Token::RParen, |parser| {
                    parser.parse_expr().context("parsing function argument")
                })?;
                self.take(Token::RParen)
                    .context("expected ')' after function arguments")?;
                let args = self.arena.alloc_slice_fill_iter(args);
                lhs = Term::App {
                    func: FunName::Term(self.arena.alloc(lhs)),
                    args,
                };
                continue;
            }

            let Some(op) = self.match_binop() else {
                break;
            };
            let prec = op.precedence();
            if prec < min_prec {
                break;
            }
            let next_min_prec = match op.assoc() {
                Assoc::Left => prec.next_level(),
                Assoc::Right => prec,
            };
            self.next();

            let rhs = self
                .parse_expr_prec(next_min_prec)
                .context("parsing right-hand side of binary expression")?;
            let rhs = self.alloc(rhs);

            let func = FunName::BinOp(op);
            let lhs_ref = self.alloc(lhs);
            let args = self.arena.alloc_slice_fill_iter([lhs_ref, rhs]);
            lhs = Term::App { func, args };
        }

        Ok(lhs)
    }

    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "unrecognised tokens are intentionally caught by the wildcard arm"
    )]
    fn match_unop(&mut self) -> Option<UnOp> {
        match self.peek()? {
            Token::Bang => Some(UnOp::Not),
            _ => None,
        }
    }

    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "unrecognised tokens are intentionally caught by the wildcard arm"
    )]
    fn match_binop(&mut self) -> Option<BinOp> {
        match self.peek()? {
            // `|` after an expression is bitwise OR (never lambda — lambdas are atoms)
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

    /// Parse a function call with arguments
    fn parse_function_call(&mut self, name: &'names Name) -> Result<Term<'names, 'ast>> {
        let args = self.parse_separated_list(Token::RParen, |parser| {
            parser.parse_expr().context("parsing function argument")
        })?;
        self.take(Token::RParen)
            .context("expected ')' after function arguments")?;
        let args = self.arena.alloc_slice_fill_iter(args);
        Ok(Term::App {
            func: FunName::Term(self.arena.alloc(Term::Var(name))),
            args,
        })
    }

    /// Parse a parenthesized expression
    fn parse_paren_expr(&mut self) -> Result<Term<'names, 'ast>> {
        let expr = self
            .parse_expr_owned()
            .context("parsing expression in parentheses")?;
        self.take(Token::RParen)
            .context("expected ')' after parenthesized expression")?;
        Ok(expr)
    }

    /// Parse a match expression
    fn parse_match_expr(&mut self) -> Result<Term<'names, 'ast>> {
        let scrutinee = self.parse_expr().context("parsing match scrutinee")?;
        self.take(Token::LBrace)
            .context("expected '{' after match expression")?;
        let arms = self.parse_match_arms()?;
        self.take(Token::RBrace)
            .context("expected '}' after match arms")?;
        let arms = self.arena.alloc_slice_fill_iter(arms);
        Ok(Term::Match { scrutinee, arms })
    }

    /// Parse a function type: `fn(params) -> ret_ty`
    ///
    /// Called after consuming the `fn` token. Each param is `name: type`.
    fn parse_fn_type(&mut self) -> Result<Term<'names, 'ast>> {
        self.take(Token::LParen)
            .context("expected '(' in function type")?;
        let params = self.parse_params()?;
        self.take(Token::RParen)
            .context("expected ')' in function type")?;
        self.take(Token::Arrow)
            .context("expected '->' in function type")?;
        let ret_ty = self
            .parse_expr()
            .context("expected return type in function type")?;
        Ok(Term::Pi { params, ret_ty })
    }

    /// Parse a lambda expression: `lam(params) (-> ret_ty)? = body`
    ///
    /// Called after consuming the `lam` token.
    fn parse_lambda(&mut self) -> Result<Term<'names, 'ast>> {
        self.take(Token::LParen)
            .context("expected '(' after 'lam'")?;
        let params_vec = self.parse_separated_list(Token::RParen, |parser| {
            let name = parser
                .take_ident()
                .context("expected parameter name in lambda")?;
            parser
                .take(Token::Colon)
                .context("expected ':' in lambda parameter (type annotations are required)")?;
            let ty = parser.parse_expr().context("expected parameter type")?;
            Ok(Param { name, ty })
        })?;
        self.take(Token::RParen)
            .context("expected ')' after lambda parameters")?;

        let ret_ty = self
            .consume_if(Token::Arrow)
            .then(|| self.parse_expr().context("expected return type after '->'"))
            .transpose()?;

        self.take(Token::Eq)
            .context("expected '=' after lambda parameters")?;
        let body = self.parse_expr().context("expected lambda body")?;
        let params = self.arena.alloc_slice_fill_iter(params_vec);
        Ok(Term::Lam {
            params,
            ret_ty,
            body,
        })
    }

    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "unrecognised tokens are intentionally caught by the wildcard arm"
    )]
    fn parse_atom_owned(&mut self) -> Result<Term<'names, 'ast>> {
        let token = self.next().context("expected expression")??;
        match token {
            Token::Num(n) => Ok(Term::Lit(n)),
            Token::Ident(name) => {
                if self.consume_if(Token::LParen) {
                    self.parse_function_call(name)
                } else {
                    Ok(Term::Var(name))
                }
            }
            // `fn` not followed by ident → function type expression
            Token::Fn => self.parse_fn_type(),
            Token::Lam => self.parse_lambda(),
            Token::LParen => self.parse_paren_expr(),
            Token::HashLParen => self.parse_quoted_expr(),
            Token::HashLBrace => self.parse_quoted_block(),
            Token::DollarLParen => self.parse_spliced_expr(),
            Token::DollarLBrace => self.parse_spliced_block(),
            Token::DoubleLBracket => self.parse_lifted_expr(),
            Token::Match => self.parse_match_expr(),
            Token::LBrace => {
                let (stmts, expr) = self.parse_block_inner()?;
                Ok(Term::Block { stmts, expr })
            }
            _ => Err(anyhow::anyhow!("unexpected token in expression: {token:?}")),
        }
    }

    fn parse_match_arms(&mut self) -> Result<Vec<MatchArm<'names, 'ast>>> {
        let mut arms = Vec::new();
        while self.peek().is_some() && !matches!(self.peek(), Some(Token::RBrace)) {
            let pat = self.parse_pattern().context("parsing match pattern")?;
            self.take(Token::DArrow)
                .context("expected '=>' in match arm")?;
            let body = self.parse_expr().context("parsing match arm body")?;
            arms.push(MatchArm { pat, body });
            // Comma is optional after the last arm
            if !self.consume_if(Token::Comma) {
                break;
            }
        }
        Ok(arms)
    }

    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "unrecognised tokens are intentionally caught by the wildcard arm"
    )]
    fn parse_pattern(&mut self) -> Result<Pat<'names>> {
        let token = self.next().context("expected pattern")??;
        match token {
            Token::Num(n) => Ok(Pat::Lit(n)),
            Token::Ident(name) => Ok(Pat::Name(name)),
            _ => Err(anyhow::anyhow!("unexpected token in pattern: {token:?}")),
        }
    }
}

#[cfg(test)]
mod test;
