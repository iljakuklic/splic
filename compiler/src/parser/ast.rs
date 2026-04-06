pub use crate::common::{Assoc, BinOp, Name, Phase, UnOp};

/// Function or operator reference
#[derive(Clone, Copy, derive_more::Debug)]
pub enum FunName<'names, 'ast> {
    #[debug("{_0:?}")]
    Term(&'ast Term<'names, 'ast>),
    #[debug("{_0:?}")]
    BinOp(BinOp),
    #[debug("{_0:?}")]
    UnOp(UnOp),
}

#[derive(derive_more::Debug)]
pub enum Pat<'names> {
    #[debug("{_0:?}")]
    Name(&'names Name),
    #[debug("{_0:?}")]
    Lit(u64),
}

#[derive(Debug)]
pub struct MatchArm<'names, 'ast> {
    pub pat: Pat<'names>,
    pub body: &'ast Term<'names, 'ast>,
}

#[derive(Debug)]
pub struct Let<'names, 'ast> {
    pub name: &'names Name,
    pub ty: Option<&'ast Term<'names, 'ast>>,
    pub expr: &'ast Term<'names, 'ast>,
}

#[derive(Debug)]
pub struct Param<'names, 'ast> {
    pub name: &'names Name,
    pub ty: &'ast Term<'names, 'ast>,
}

#[derive(Debug)]
pub struct Function<'names, 'ast> {
    pub phase: Phase,
    pub name: &'names Name,
    pub params: &'ast [Param<'names, 'ast>],
    pub ret_ty: &'ast Term<'names, 'ast>,
    pub body: &'ast Term<'names, 'ast>,
}

#[derive(Debug)]
pub struct Program<'names, 'ast> {
    pub functions: &'ast [Function<'names, 'ast>],
}

#[derive(derive_more::Debug)]
pub enum Term<'names, 'ast> {
    #[debug("{_0:?}")]
    Lit(u64),

    #[debug("{_0:?}")]
    Var(&'names Name),

    App {
        func: FunName<'names, 'ast>,
        args: &'ast [&'ast Self],
    },

    /// Function type: `fn(name: ty, ...) -> ret_ty`
    Pi {
        params: &'ast [Param<'names, 'ast>],
        ret_ty: &'ast Self,
    },

    /// Lambda: `|params| body`
    Lam {
        params: &'ast [Param<'names, 'ast>],
        body: &'ast Self,
    },

    Quote(&'ast Self),

    Splice(&'ast Self),

    Lift(&'ast Self),

    Match {
        scrutinee: &'ast Self,
        arms: &'ast [MatchArm<'names, 'ast>],
    },

    Block {
        stmts: &'ast [Let<'names, 'ast>],
        expr: &'ast Self,
    },
}
