pub mod pretty;
mod prim;
mod subst;

pub mod alpha_eq;
pub use crate::common::{Name, Phase};
pub use alpha_eq::alpha_eq;
pub use prim::{IntType, IntWidth, Prim};
pub use subst::subst;

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

/// Elaborated top-level function definition.
///
/// `ty` is always a `Term::Pi`; use `Function::pi()` for convenient access.
#[derive(Debug)]
pub struct Function<'a> {
    pub name: Name<'a>,
    /// Function type (always `Term::Pi`). The Pi carries the phase, params, and return type.
    pub ty: &'a Term<'a>,
    pub body: &'a Term<'a>,
}

impl<'a> Function<'a> {
    /// Unwrap `self.ty` as a `Pi`. Panics if `ty` is not a `Pi` (typechecker invariant).
    pub fn pi(&self) -> &Pi<'a> {
        match self.ty {
            Term::Pi(pi) => pi,
            _ => unreachable!("Function::ty must be a Pi (typechecker invariant)"),
        }
    }
}

/// Elaborated program: a sequence of top-level function definitions
#[derive(Debug)]
pub struct Program<'a> {
    pub functions: &'a [Function<'a>],
}

/// Function or primitive application: `func(args...)`
///
/// `func` may be any term yielding a function type — most commonly:
/// - `Term::Global(name)` for top-level function calls
/// - `Term::Prim(p)` for built-in primitive operations
/// - any expression for higher-order calls
///
/// An empty `args` slice represents a zero-argument call and is distinct from
/// a bare reference to `func`.
#[derive(Debug, PartialEq, Eq)]
pub struct App<'a> {
    pub func: &'a Term<'a>,
    pub args: &'a [&'a Term<'a>],
}

/// Dependent function type: `fn(params...) -> body_ty`
///
/// `phase` distinguishes meta-level (`fn`) from object-level (`code fn`) functions.
/// This allows the globals table to store `&Term` directly, unifying type lookup
/// for globals and locals.
#[derive(Debug, PartialEq, Eq)]
pub struct Pi<'a> {
    pub params: &'a [(&'a str, &'a Term<'a>)], // (name, type) pairs
    pub body_ty: &'a Term<'a>,
    pub phase: Phase,
}

/// Lambda abstraction: |params...| body
#[derive(Debug, PartialEq, Eq)]
pub struct Lam<'a> {
    pub params: &'a [(&'a str, &'a Term<'a>)], // (name, type) pairs
    pub body: &'a Term<'a>,
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
    /// Built-in type or operation (not applied)
    Prim(Prim),
    /// Numeric literal with its integer type
    Lit(u64, IntType),
    /// Global function reference
    Global(Name<'a>),
    /// Function or primitive application: func(args...)
    App(App<'a>),
    /// Dependent function type: fn(x: A) -> B
    Pi(Pi<'a>),
    /// Lambda abstraction: |x: A| body
    Lam(Lam<'a>),
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

impl Term<'static> {
    // Integer types at meta phase
    pub const U0_META: Self = Self::Prim(Prim::IntTy(IntType::U0_META));
    pub const U1_META: Self = Self::Prim(Prim::IntTy(IntType::U1_META));
    pub const U8_META: Self = Self::Prim(Prim::IntTy(IntType::U8_META));
    pub const U16_META: Self = Self::Prim(Prim::IntTy(IntType::U16_META));
    pub const U32_META: Self = Self::Prim(Prim::IntTy(IntType::U32_META));
    pub const U64_META: Self = Self::Prim(Prim::IntTy(IntType::U64_META));

    // Integer types at object phase
    pub const U0_OBJ: Self = Self::Prim(Prim::IntTy(IntType::U0_OBJ));
    pub const U1_OBJ: Self = Self::Prim(Prim::IntTy(IntType::U1_OBJ));
    pub const U8_OBJ: Self = Self::Prim(Prim::IntTy(IntType::U8_OBJ));
    pub const U16_OBJ: Self = Self::Prim(Prim::IntTy(IntType::U16_OBJ));
    pub const U32_OBJ: Self = Self::Prim(Prim::IntTy(IntType::U32_OBJ));
    pub const U64_OBJ: Self = Self::Prim(Prim::IntTy(IntType::U64_OBJ));

    // Universes
    pub const TYPE: Self = Self::Prim(Prim::U(Phase::Meta));
    pub const VM_TYPE: Self = Self::Prim(Prim::U(Phase::Object));

    /// Return the static integer-type term for the given width and phase.
    pub const fn int_ty(width: IntWidth, phase: Phase) -> &'static Self {
        match (width, phase) {
            (IntWidth::U0, Phase::Meta) => &Self::U0_META,
            (IntWidth::U1, Phase::Meta) => &Self::U1_META,
            (IntWidth::U8, Phase::Meta) => &Self::U8_META,
            (IntWidth::U16, Phase::Meta) => &Self::U16_META,
            (IntWidth::U32, Phase::Meta) => &Self::U32_META,
            (IntWidth::U64, Phase::Meta) => &Self::U64_META,
            (IntWidth::U0, Phase::Object) => &Self::U0_OBJ,
            (IntWidth::U1, Phase::Object) => &Self::U1_OBJ,
            (IntWidth::U8, Phase::Object) => &Self::U8_OBJ,
            (IntWidth::U16, Phase::Object) => &Self::U16_OBJ,
            (IntWidth::U32, Phase::Object) => &Self::U32_OBJ,
            (IntWidth::U64, Phase::Object) => &Self::U64_OBJ,
        }
    }

    /// Return the static u1 term for the given phase.
    pub const fn u1_ty(phase: Phase) -> &'static Self {
        match phase {
            Phase::Meta => &Self::U1_META,
            Phase::Object => &Self::U1_OBJ,
        }
    }

    /// Return the universe term for the given phase (`Type` or `VmType`).
    pub const fn universe(phase: Phase) -> &'static Self {
        match phase {
            Phase::Meta => &Self::TYPE,
            Phase::Object => &Self::VM_TYPE,
        }
    }
}

impl<'a> Term<'a> {
    pub const fn new_app(func: &'a Self, args: &'a [&'a Self]) -> Self {
        Self::App(App { func, args })
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

impl<'a> From<Let<'a>> for Term<'a> {
    fn from(let_: Let<'a>) -> Self {
        Self::Let(let_)
    }
}

impl<'a> From<App<'a>> for Term<'a> {
    fn from(app: App<'a>) -> Self {
        Self::App(app)
    }
}

impl<'a> From<Match<'a>> for Term<'a> {
    fn from(match_: Match<'a>) -> Self {
        Self::Match(match_)
    }
}
