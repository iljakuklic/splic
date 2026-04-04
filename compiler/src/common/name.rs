#[derive(
    PartialEq,
    Eq,
    Hash,
    ref_cast::RefCastCustom,
    derive_more::Display,
    derive_more::Debug,
    derive_more::AsRef,
)]
#[display("{_0}")]
#[debug("{_0:?}")]
#[repr(transparent)]
pub struct Name(#[as_ref(str)] str);

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
