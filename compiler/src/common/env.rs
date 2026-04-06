use std::ops::{Index, IndexMut};

use derive_more::Debug;

use super::de_bruijn::{Depth, Lvl};

/// A level-indexed environment: entries stored oldest-first.
///
/// `get(Lvl(i))` returns `entries[i]`.  `depth()` returns the number of
/// entries as a [`Depth`].  All mutations go through typed accessors so
/// callers never have to think about the oldest-first layout.
#[derive(Clone, Debug, Default)]
pub struct Env<T> {
    entries: Vec<T>,
}

impl<T> Env<T> {
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: Vec::with_capacity(capacity),
        }
    }

    /// Number of entries, expressed as a De Bruijn depth.
    pub const fn depth(&self) -> Depth {
        Depth::new(self.entries.len())
    }

    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Append an entry; it becomes the new innermost binding.
    pub fn push(&mut self, val: T) {
        self.entries.push(val);
    }

    /// Remove and return the innermost entry.
    ///
    /// # Panics
    /// Panics if the environment is empty.
    pub fn pop(&mut self) -> T {
        self.entries.pop().expect("pop on empty Env")
    }

    /// Retrieve the entry at De Bruijn level `lvl`.
    ///
    /// # Panics
    /// Panics if `lvl` is out of bounds.
    pub fn get(&self, lvl: Lvl) -> &T {
        self.entries
            .get(lvl.as_usize())
            .expect("De Bruijn level out of environment bounds")
    }

    /// Mutable access to the entry at level `lvl`.
    ///
    /// # Panics
    /// Panics if `lvl` is out of bounds.
    pub fn get_mut(&mut self, lvl: Lvl) -> &mut T {
        self.entries
            .get_mut(lvl.as_usize())
            .expect("De Bruijn level out of environment bounds")
    }

    /// Truncate to `depth` entries, removing all entries beyond that depth.
    pub fn truncate(&mut self, depth: Depth) {
        self.entries.truncate(depth.as_usize());
    }

    /// Extend with the entries produced by `iter`.
    pub fn extend(&mut self, iter: impl IntoIterator<Item = T>) {
        self.entries.extend(iter);
    }

    /// A slice view of all entries (oldest first).
    pub fn as_slice(&self) -> &[T] {
        &self.entries
    }

    /// Iterate over entries oldest-first.  Supports `.rev()` for innermost-first.
    #[expect(
        clippy::iter_without_into_iter,
        reason = "IntoIterator not needed here"
    )]
    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.entries.iter()
    }

    #[expect(
        clippy::iter_without_into_iter,
        reason = "IntoIterator not needed here"
    )]
    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, T> {
        self.entries.iter_mut()
    }

    /// Iterate over `(level, entry)` pairs.
    pub fn iter_with_lvl(&self) -> impl Iterator<Item = (Lvl, &T)> {
        self.entries
            .iter()
            .enumerate()
            .map(|(i, v)| (Lvl::new(i), v))
    }
}

impl<T> Index<Lvl> for Env<T> {
    type Output = T;

    fn index(&self, lvl: Lvl) -> &T {
        self.get(lvl)
    }
}

impl<T> IndexMut<Lvl> for Env<T> {
    fn index_mut(&mut self, lvl: Lvl) -> &mut T {
        self.get_mut(lvl)
    }
}
