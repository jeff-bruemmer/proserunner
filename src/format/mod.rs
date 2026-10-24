//! Output formats. Every format prints the same numbered report.

mod checkstyle;
mod github;
mod group;
mod json;
mod located;
mod output;
mod plain;
mod sarif;
mod table;
mod verbose;

pub use output::{Output, write};
