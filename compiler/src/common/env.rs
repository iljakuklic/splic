use std::ops::{Index, IndexMut};

use derive_more::Debug;

use super::de_bruijn::{Depth, Ix, Lvl};

/// A level-indexed environment: entries stored oldest-first.
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
    pub fn pop(&mut self) -> T {
        self.entries.pop().expect("pop on empty Env")
    }

    /// Retrieve the entry at De Bruijn level `lvl`.
    pub fn get_at_lvl(&self, lvl: Lvl) -> &T {
        self.entries
            .get(lvl.as_usize())
            .expect("De Bruijn level out of environment bounds")
    }

    /// Mutable access to the entry at level `lvl`.
    pub fn get_at_lvl_mut(&mut self, lvl: Lvl) -> &mut T {
        self.entries
            .get_mut(lvl.as_usize())
            .expect("De Bruijn level out of environment bounds")
    }

    /// Convert De Bruijn index to level.
    pub const fn ix_to_lvl(&self, ix: Ix) -> Lvl {
        ix.lvl_at(self.depth())
    }

    /// Convert De Bruijn level to index.
    pub const fn lvl_to_ix(&self, lvl: Lvl) -> Ix {
        lvl.ix_at(self.depth())
    }

    /// Retrieve the entry at De Bruijn index `ix`.
    pub fn get_at_ix(&self, ix: Ix) -> &T {
        self.get_at_lvl(self.ix_to_lvl(ix))
    }

    /// Mutable access to the entry at De Bruijn index `ix`.
    pub fn get_at_ix_mut(&mut self, ix: Ix) -> &mut T {
        let lvl = self.ix_to_lvl(ix);
        self.get_at_lvl_mut(lvl)
    }

    /// Truncate to `depth` entries, removing all entries beyond that depth.
    pub fn truncate(&mut self, depth: Depth) {
        self.entries.truncate(depth.as_usize());
    }

    /// A slice view of all entries (oldest first).
    pub fn as_slice(&self) -> &[T] {
        &self.entries
    }

    /// Iterate over entries oldest-first (by De Bruijn level).  Supports `.rev()` for innermost-first.
    pub fn iter_by_lvl(&self) -> std::slice::Iter<'_, T> {
        self.entries.iter()
    }

    /// Search innermost-first; return `(lvl, ix, entry)` for the first match.
    pub fn find_innermost<F: Fn(&T) -> bool>(&self, pred: F) -> Option<(Lvl, Ix, &T)> {
        for (i, entry) in self.entries.iter().enumerate().rev() {
            if pred(entry) {
                let lvl = Lvl::new(i);
                let ix = lvl.ix_at(self.depth());
                return Some((lvl, ix, entry));
            }
        }
        None
    }

    /// Look up the innermost entry whose name (extracted via `key`) equals `name`.
    pub fn lookup<N, F>(&self, name: &N, key: F) -> Option<(Lvl, Ix, &T)>
    where
        N: PartialEq + ?Sized,
        F: Fn(&T) -> &N,
    {
        self.find_innermost(|e| key(e) == name)
    }

    /// Iterate over `(level, entry)` pairs.
    pub fn iter_with_lvl(&self) -> impl DoubleEndedIterator<Item = (Lvl, &T)> + ExactSizeIterator {
        self.entries
            .iter()
            .enumerate()
            .map(|(i, v)| (Lvl::new(i), v))
    }
}

impl<T> std::iter::Extend<T> for Env<T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        self.entries.extend(iter);
    }
}

impl<T> FromIterator<T> for Env<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Self {
            entries: Vec::from_iter(iter),
        }
    }
}

impl<T> Index<Lvl> for Env<T> {
    type Output = T;

    fn index(&self, lvl: Lvl) -> &T {
        self.get_at_lvl(lvl)
    }
}

impl<T> IndexMut<Lvl> for Env<T> {
    fn index_mut(&mut self, lvl: Lvl) -> &mut T {
        self.get_at_lvl_mut(lvl)
    }
}

impl<T> Index<Ix> for Env<T> {
    type Output = T;

    fn index(&self, ix: Ix) -> &T {
        self.get_at_ix(ix)
    }
}

impl<T> IndexMut<Ix> for Env<T> {
    fn index_mut(&mut self, ix: Ix) -> &mut T {
        self.get_at_ix_mut(ix)
    }
}
