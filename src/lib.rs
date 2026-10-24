//! Proserunner: a fast, customizable prose linter.
//!
//! ```text
//! walk → read → scan → lint → policy → report → format
//!  I/O    I/O   pure   pure    pure      pure     I/O
//! ```
//!
//! `scan`, `lint`, `policy` and `report` do no I/O, so editors and other
//! tools can use them directly. `config` and `ignores` read and
//! write settings; `ctx` says where.
//!
//! # Example
//!
//! Checks a Markdown string with the built-in rules and default settings,
//! and prints its issues as `proserunner check -o plain` would:
//!
//! ```
//! use std::path::Path;
//!
//! use proserunner::config::Config;
//! use proserunner::format::{self, Output};
//! use proserunner::ignores::Ignores;
//! use proserunner::lint;
//! use proserunner::policy::Policy;
//! use proserunner::report::{self, Document};
//! use proserunner::rules::{self, RuleSet};
//! use proserunner::scan::{Format, Options};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let compiled = RuleSet::compile(rules::builtin());
//! assert!(compiled.errors.is_empty());
//! let rules = compiled.rules;
//! let policy = Policy::try_new(&Config::default(), &rules, Ignores::default())?;
//!
//! let source = "We utilize the the tools.\n";
//! let linted = lint::document(&rules, Format::Markdown, source, Options::default());
//! let applied = policy.apply(None, source, linted)?;
//! let documents = [Document::new("notes.md".into(), None, source.into(), applied.issues)];
//! let report = report::build(&rules, &documents);
//!
//! let mut out = Vec::new();
//! format::write(&mut out, &report, Output::Plain, Path::new("."))?;
//! assert_eq!(
//!     String::from_utf8(out)?,
//!     "notes.md:1:4: [1] \"utilize\" -> Prefer: use. (default/needless-variants)\n\
//!      notes.md:1:12: [2] \"the the\" -> Repeated word: delete one. (default/repetition)\n"
//! );
//! # Ok(())
//! # }
//! ```

pub mod config;
pub mod ctx;
#[doc(hidden)]
pub mod english;
pub mod error;
pub mod examples;
pub mod explain;
pub mod format;
#[doc(hidden)]
pub mod fsutil;
pub mod ignores;
pub mod lint;
#[doc(hidden)]
#[allow(clippy::exhaustive_enums, reason = "binary support, not public API")]
pub mod migrate;
pub mod policy;
pub mod prose;
pub mod report;
pub mod rules;
pub mod scan;
mod settings;
mod typo;
pub mod walk;

#[cfg(test)]
mod json_schemas;

pub use error::{Error, Result};
