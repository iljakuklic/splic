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
    pub fn ix_at(self, depth: Depth) -> Ix {
        let result = depth
            .0
            .checked_sub(self.0 + 1)
            .expect("De Bruijn level out of range for depth (level must be < depth)");
        Ix(result)
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
    pub fn lvl_at(self, depth: Depth) -> Self {
        let result = depth
            .0
            .checked_sub(self.0 + 1)
            .expect("De Bruijn index out of range for depth (index must be < depth)");
        Self(result)
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
