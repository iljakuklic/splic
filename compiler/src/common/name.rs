#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Name<'a>(pub &'a str);

impl<'a> Name<'a> {
    pub const fn new(s: &'a str) -> Self {
        Name(s)
    }

    pub const fn as_str(self) -> &'a str {
        self.0
    }
}

impl std::fmt::Display for Name<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::fmt::Debug for Name<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
