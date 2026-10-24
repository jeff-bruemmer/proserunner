//! Pieces: the runs of source text a scanner gathers for a block.

/// A run of source text, and the source offset it starts at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Piece<'src> {
    pub at: usize,
    pub text: &'src str,
}
