pub use crate::common::{Assoc, BinOp, Name, Phase, UnOp};

/// Function or operator reference
#[derive(Clone, Copy, derive_more::Debug)]
pub enum FunName<'a> {
    #[debug("{_0:?}")]
    Term(&'a Term<'a>),
    #[debug("{_0:?}")]
    BinOp(BinOp),
    #[debug("{_0:?}")]
    UnOp(UnOp),
}

#[derive(Debug)]
pub enum Pat<'a> {
    Name(&'a Name),
    Lit(u64),
}

#[derive(Debug)]
pub struct MatchArm<'a> {
    pub pat: Pat<'a>,
    pub body: &'a Term<'a>,
}

#[derive(Debug)]
pub struct Let<'a> {
    pub name: &'a Name,
    pub ty: Option<&'a Term<'a>>,
    pub expr: &'a Term<'a>,
}

#[derive(Debug)]
pub struct Param<'a> {
    pub name: &'a Name,
    pub ty: &'a Term<'a>,
}

#[derive(Debug)]
pub struct Function<'a> {
    pub phase: Phase,
    pub name: &'a Name,
    pub params: &'a [Param<'a>],
    pub ret_ty: &'a Term<'a>,
    pub body: &'a Term<'a>,
}

#[derive(Debug)]
pub struct Program<'a> {
    pub functions: &'a [Function<'a>],
}

#[derive(Debug)]
pub enum Term<'a> {
    Lit(u64),
    Var(&'a Name),
    App {
        func: FunName<'a>,
        args: &'a [&'a Self],
    },
    /// Function type: `fn(name: ty, ...) -> ret_ty`
    Pi {
        params: &'a [Param<'a>],
        ret_ty: &'a Self,
    },
    /// Lambda: `|params| body`
    Lam {
        params: &'a [Param<'a>],
        body: &'a Self,
    },
    Quote(&'a Self),
    Splice(&'a Self),
    Lift(&'a Self),
    Match {
        scrutinee: &'a Self,
        arms: &'a [MatchArm<'a>],
    },
    Block {
        stmts: &'a [Let<'a>],
        expr: &'a Self,
    },
}
