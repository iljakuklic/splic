/// Compilation phase
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, derive_more::Display, derive_more::IsVariant)]
pub enum Phase {
    #[display("meta")]
    Meta,
    #[display("object")]
    Object,
}
