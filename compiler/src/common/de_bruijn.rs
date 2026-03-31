/// De Bruijn level (counts from the outermost binder, 0 = outermost)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Lvl(usize);

impl Lvl {
    pub const fn new(n: usize) -> Self {
        Self(n)
    }

    #[must_use]
    pub const fn as_usize(self) -> usize {
        self.0
    }

    #[must_use]
    pub const fn succ(self) -> Self {
        Self(self.0 + 1)
    }

    #[must_use]
    pub const fn ix_at_depth(self, depth: Depth) -> Ix {
        Ix(depth.0 - self.0 - 1)
    }
}

/// De Bruijn index (counts from nearest enclosing binder, 0 = innermost)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Ix(usize);

impl Ix {
    pub const fn new(n: usize) -> Self {
        Self(n)
    }

    #[must_use]
    pub const fn as_usize(self) -> usize {
        self.0
    }

    #[must_use]
    pub const fn succ(self) -> Self {
        Self(self.0 + 1)
    }

    #[must_use]
    pub const fn lvl_at_depth(self, depth: Depth) -> Self {
        Self(depth.0 - self.0 - 1)
    }
}

/// De Bruijn depth (counts the number of binders from outermost one, 0 = no binders).
///
/// Same as `Lvl` but used to count how many binders down the current expression is,
/// not to index into environment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Depth(usize);

impl Depth {
    pub const ZERO: Self = Self(0);

    pub const fn new(n: usize) -> Self {
        Self(n)
    }

    #[must_use]
    pub const fn as_usize(self) -> usize {
        self.0
    }

    #[must_use]
    pub const fn as_lvl(self) -> Lvl {
        Lvl::new(self.0)
    }

    #[must_use]
    pub const fn succ(self) -> Self {
        Self(self.0 + 1)
    }
}
