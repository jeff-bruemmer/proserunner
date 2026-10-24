//! What a check's hooks take and give: what it asks the token tier to
//! find, what that found, and what the check found in turn.

use std::ops::Range;

use crate::rules::RuleIndex;
use crate::rules::rule_file::Case;
use crate::scan::Block;

/// An issue a check found in a block.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Found {
    /// Bytes of the block's matching view.
    pub range: Range<usize>,
    /// Values for the check's own placeholders, in [`Spec::vars`] order.
    pub args: Vec<String>,
}

/// An issue a check's `document` hook found, and the block it's in.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct FoundIn {
    /// Index into the document's blocks.
    pub block: u32,
    pub found: Found,
}

/// Something a check asks the token tier to find for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Want {
    /// Matched like one word of a `words` entry, without brace
    /// expansion.
    Literal(String, Case),
    /// A regex as written. Its groups come with each match.
    Pattern(String),
}

impl Want {
    /// As written, for warnings.
    pub(crate) fn into_source(self) -> String {
        match self {
            Want::Literal(text, _) | Want::Pattern(text) => text,
        }
    }
}

/// What the token tier found for a check's [`Want`].
#[derive(Debug, Clone)]
pub(crate) struct Candidate<'block> {
    pub rule: RuleIndex,
    /// Index into the rule's checks.
    pub check: u32,
    /// Index into the check's wants.
    pub tag: u32,
    /// Index into the document's blocks.
    pub block: u32,
    /// The source offset where it starts, which orders candidates.
    pub at: usize,
    /// Bytes of the block's matching view.
    pub range: Range<usize>,
    pub text: &'block str,
    /// A pattern's groups: `{1}` to `{n}`, then named groups.
    pub args: Vec<String>,
}

/// A block a rule applies to, and its index in the document.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Covered<'doc> {
    pub index: u32,
    pub block: &'doc Block,
}

/// What a check's `document` hook sees.
pub(crate) struct Doc<'doc> {
    /// The blocks its rule applies to, in document order.
    pub blocks: &'doc [Covered<'doc>],
    /// What its wants found in those blocks, in document order, without
    /// the rule's exceptions.
    pub candidates: &'doc [Candidate<'doc>],
}
