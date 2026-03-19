pub mod pretty;
mod prim;

pub use crate::parser::ast::{Name, Phase};
pub use prim::{IntType, IntWidth, Prim};

/// De Bruijn level (counts from the outermost binder)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Lvl(pub usize);

impl Lvl {
    pub const fn new(n: usize) -> Self {
        Self(n)
    }

    #[must_use]
    pub const fn succ(self) -> Self {
        Self(self.0 + 1)
    }
}

/// Head of an application: either a top-level function or a primitive op
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Head<'a> {
    Global(Name<'a>), // resolved top-level function name
    Prim(Prim),       // built-in operation with resolved width
}

impl Head<'_> {
    /// Returns `true` if this head is a binary infix primitive operator.
    pub const fn is_binop(&self) -> bool {
        matches!(self, Self::Prim(p) if p.is_binop())
    }
}

/// Match pattern in the core IR
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pat<'a> {
    Lit(u64),
    Bind(&'a str), // named binding
    Wildcard,      // _ pattern
}

impl<'a> Pat<'a> {
    /// Return the name bound by this pattern, if any.
    pub const fn bound_name(&self) -> Option<&'a str> {
        match self {
            Pat::Bind(name) => Some(name),
            Pat::Lit(_) | Pat::Wildcard => None,
        }
    }
}

/// Match arm
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Arm<'a> {
    pub pat: Pat<'a>,
    pub body: &'a Term<'a>,
}

/// Top-level function signature (stored in the globals table during elaboration)
#[derive(Debug)]
pub struct FunSig<'a> {
    pub params: &'a [(&'a str, &'a Term<'a>)], // (name, type) pairs
    pub ret_ty: &'a Term<'a>,
    pub phase: Phase,
}

/// Elaborated top-level function definition
#[derive(Debug)]
pub struct Function<'a> {
    pub name: Name<'a>,
    pub sig: FunSig<'a>,
    pub body: &'a Term<'a>,
}

/// Elaborated program: a sequence of top-level function definitions
#[derive(Debug)]
pub struct Program<'a> {
    pub functions: &'a [Function<'a>],
}

/// Application of a global function or primitive operation to arguments.
#[derive(Debug, PartialEq, Eq)]
pub struct App<'a> {
    pub head: Head<'a>,
    pub args: &'a [&'a Term<'a>],
}

impl App<'_> {
    /// Returns the number of arguments.
    pub const fn arity(&self) -> usize {
        self.args.len()
    }

    /// Returns `true` if this application is a binary infix primitive operator.
    ///
    /// Asserts that the argument count is exactly 2, which is an invariant
    /// enforced by the elaborator for all binop applications.
    pub fn is_binop(&self) -> bool {
        let result = self.head.is_binop();
        if result {
            assert_eq!(self.arity(), 2, "binop App must have exactly 2 arguments");
        }
        result
    }
}

/// Let binding with explicit type annotation and a body.
#[derive(Debug, PartialEq, Eq)]
pub struct Let<'a> {
    pub name: &'a str,
    pub ty: &'a Term<'a>,
    pub expr: &'a Term<'a>,
    pub body: &'a Term<'a>,
}

/// Pattern match.
#[derive(Debug, PartialEq, Eq)]
pub struct Match<'a> {
    pub scrutinee: &'a Term<'a>,
    pub arms: &'a [Arm<'a>],
}

/// Core term / type (terms and types are unified)
#[derive(Debug, PartialEq, Eq)]
pub enum Term<'a> {
    /// Local variable, identified by De Bruijn level
    Var(Lvl),
    /// Built-in type or operation
    Prim(Prim),
    /// Numeric literal
    Lit(u64),
    /// Application of a global function or primitive operation to arguments
    App(App<'a>),
    /// Lift: [[T]] — meta type representing object-level code of type T
    Lift(&'a Self),
    /// Quotation: #(t) — produce object-level code from a meta expression
    Quote(&'a Self),
    /// Splice: $(t) — run meta code and insert result into object context
    Splice(&'a Self),
    /// Let binding with explicit type annotation and a body
    Let(Let<'a>),
    /// Pattern match
    Match(Match<'a>),
}

impl<'a> Term<'a> {
    pub const fn new_app(head: Head<'a>, args: &'a [&'a Self]) -> Self {
        Self::App(App { head, args })
    }

    pub const fn new_let(name: &'a str, ty: &'a Self, expr: &'a Self, body: &'a Self) -> Self {
        Self::Let(Let {
            name,
            ty,
            expr,
            body,
        })
    }

    pub const fn new_match(scrutinee: &'a Self, arms: &'a [Arm<'a>]) -> Self {
        Self::Match(Match { scrutinee, arms })
    }
}

impl<'a> From<App<'a>> for Term<'a> {
    fn from(app: App<'a>) -> Self {
        Self::App(app)
    }
}

impl<'a> From<Let<'a>> for Term<'a> {
    fn from(let_: Let<'a>) -> Self {
        Self::Let(let_)
    }
}

impl<'a> From<Match<'a>> for Term<'a> {
    fn from(match_: Match<'a>) -> Self {
        Self::Match(match_)
    }
}
