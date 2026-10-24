//! Rules: the file format, compiling, the built-ins, and rule sources.

mod builtin;
pub(crate) mod checks;
mod expand;
mod rule;
mod rule_file;
mod rule_set;
mod rule_source;
mod template;

pub use builtin::{BuiltinFile, DEFAULT_SOURCE, builtin, builtin_files};
pub use rule::{CompiledEntry, Engine, EntryKind, EntryRef, Exception, Form, Rule, Wording};
pub use rule_file::{
    AcronymsTable, CapitalizationTable, Case, Entry, Examples, Except, Fix, Formula, Level,
    LimitTable, MetricPer, MetricTable, Per, RuleFile, SCHEMA, Style, Term, Unit,
};
pub(crate) use rule_set::Target;
#[cfg(test)]
pub(crate) use rule_set::compile_one;
pub use rule_set::{Compiled, RuleIndex, RuleSet};
pub use rule_source::{RuleSource, load_dir};
pub use template::Template;
