/// Compilation phase
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, derive_more::Display)]
pub enum Phase {
    #[display("meta")]
    Meta,
    #[display("object")]
    Object,
}
