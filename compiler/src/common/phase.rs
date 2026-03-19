/// Compilation phase
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Phase {
    Meta,
    Object,
}

impl std::fmt::Display for Phase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Meta => f.write_str("meta"),
            Self::Object => f.write_str("object"),
        }
    }
}
