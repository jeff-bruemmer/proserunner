//! The check tier: what a rule finds beyond literal words and patterns,
//! such as a word repeated right after itself, or a sentence too long.
//!
//! Each check is a file here that holds its [`Spec`], its table in a rule
//! file, how it compiles, its hooks, how `explain` describes it, and its
//! tests. Adding one touches that file, `check.rs` (a variant and its match
//! arms), and its key in `rules/rule_file.rs`.
//!
//! A check has up to three hooks:
//! - `wants`: words and patterns for the token tier to find, in the same
//!   pass as every rule's entries. What they find goes to `document`, not
//!   straight to the report.
//! - `block`: issues in one block.
//! - `document`: issues across a whole document, from the blocks its rule
//!   applies to and what its wants found there, in document order.

mod acronyms;
mod capitalization;
mod check;
mod consistent;
mod hooks;
mod limit;
mod metric;
mod repeat;
mod spec;

pub use acronyms::AcronymsTable;
pub use capitalization::{CapitalizationTable, Style};
pub use limit::{LimitTable, Per, Unit};
pub use metric::{Formula, MetricPer, MetricTable};

pub(crate) use check::{Check, compile, keys};
pub(crate) use hooks::{Candidate, Covered, Doc, Found, FoundIn, Want};
pub(crate) use spec::{Description, Spec};
