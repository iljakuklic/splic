pub use crate::common::{Assoc, BinOp, Name, Phase, UnOp};

/// Function or operator reference
#[derive(Clone, Copy, derive_more::Debug)]
pub enum FunName<'n, 'a> {
    #[debug("{_0:?}")]
    Term(&'a Term<'n, 'a>),
    #[debug("{_0:?}")]
    BinOp(BinOp),
    #[debug("{_0:?}")]
    UnOp(UnOp),
}

#[derive(derive_more::Debug)]
pub enum Pat<'n> {
    #[debug("{_0:?}")]
    Name(&'n Name),
    #[debug("{_0:?}")]
    Lit(u64),
}

#[derive(Debug)]
pub struct MatchArm<'n, 'a> {
    pub pat: Pat<'n>,
    pub body: &'a Term<'n, 'a>,
}

#[derive(Debug)]
pub struct Let<'n, 'a> {
    pub name: &'n Name,
    pub ty: Option<&'a Term<'n, 'a>>,
    pub expr: &'a Term<'n, 'a>,
}

#[derive(Debug)]
pub struct Param<'n, 'a> {
    pub name: &'n Name,
    pub ty: &'a Term<'n, 'a>,
}

#[derive(Debug)]
pub struct Function<'n, 'a> {
    pub phase: Phase,
    pub name: &'n Name,
    pub params: &'a [Param<'n, 'a>],
    pub ret_ty: &'a Term<'n, 'a>,
    pub body: &'a Term<'n, 'a>,
}

#[derive(Debug)]
pub struct Program<'n, 'a> {
    pub functions: &'a [Function<'n, 'a>],
}

#[derive(derive_more::Debug)]
pub enum Term<'n, 'a> {
    #[debug("{_0:?}")]
    Lit(u64),

    #[debug("{_0:?}")]
    Var(&'n Name),

    App {
        func: FunName<'n, 'a>,
        args: &'a [&'a Self],
    },

    /// Function type: `fn(name: ty, ...) -> ret_ty`
    Pi {
        params: &'a [Param<'n, 'a>],
        ret_ty: &'a Self,
    },

    /// Lambda: `|params| body`
    Lam {
        params: &'a [Param<'n, 'a>],
        body: &'a Self,
    },

    Quote(&'a Self),

    Splice(&'a Self),

    Lift(&'a Self),

    Match {
        scrutinee: &'a Self,
        arms: &'a [MatchArm<'n, 'a>],
    },

    Block {
        stmts: &'a [Let<'n, 'a>],
        expr: &'a Self,
    },
}
