#[derive(PartialEq, Eq, Hash, ref_cast::RefCastCustom)]
#[repr(transparent)]
pub struct Name(str);

impl Name {
    #[ref_cast::ref_cast_custom]
    const fn new_unchecked(s: &str) -> &Self;

    pub const fn new(n: &str) -> &Self {
        assert!(!n.is_empty(), "Empty name");
        Self::new_unchecked(n)
    }

    pub const fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for Name {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::borrow::Borrow<str> for Name {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl<'a> From<&'a str> for &'a Name {
    fn from(s: &'a str) -> Self {
        Name::new(s)
    }
}

impl std::fmt::Display for Name {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::fmt::Debug for Name {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
