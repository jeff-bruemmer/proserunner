//! `guide` and `schema`: the docs and the JSON Schemas, built
//! into the binary, so they match the version installed and need no
//! network or checkout.

use std::fmt::Write as _;

use clap::ValueEnum;
use proserunner::Result;

use super::{EXIT_OK, console};
use crate::cli::{SchemaKind, Topic};

/// A topic's page, as Markdown.
fn page(topic: Topic) -> &'static str {
    match topic {
        Topic::Overview => include_str!("../../README.md"),
        Topic::Config => include_str!("../../docs/config.md"),
        Topic::Rules => include_str!("../../docs/rules.md"),
        Topic::Formats => include_str!("../../docs/formats.md"),
        Topic::Upgrading => include_str!("../../docs/upgrading.md"),
    }
}

/// The JSON Schema for a kind of file.
fn schema(kind: SchemaKind) -> &'static str {
    match kind {
        SchemaKind::Config => include_str!("../../schemas/config.schema.json"),
        SchemaKind::Rule => include_str!("../../schemas/rule.schema.json"),
        SchemaKind::Ignores => include_str!("../../schemas/ignores.schema.json"),
    }
}

/// Prints a topic's page as Markdown, or with none, the topics.
pub(crate) fn guide_command(topic: Option<Topic>) -> Result<u8> {
    let text = match topic {
        Some(topic) => page(topic).to_owned(),
        None => topics(),
    };
    console::stdout(|out| out.write_all(text.as_bytes()))?;
    Ok(EXIT_OK)
}

/// Each topic's name and what it covers.
fn topics() -> String {
    let rows: Vec<(String, String)> = Topic::value_variants()
        .iter()
        .filter_map(|t| {
            let value = t.to_possible_value()?;
            let help = value
                .get_help()
                .map(ToString::to_string)
                .unwrap_or_default();
            Some((value.get_name().to_owned(), help))
        })
        .collect();
    let width = rows.iter().map(|(name, _)| name.len()).max().unwrap_or(0);
    let mut out = String::from("'proserunner guide TOPIC' prints one of these, as Markdown:\n");
    for (name, help) in rows {
        let _ = writeln!(out, "  {name:<width$}  {help}");
    }
    out
}

/// Prints the JSON Schema for a kind of file.
pub(crate) fn schema_command(kind: SchemaKind) -> Result<u8> {
    console::stdout(|out| out.write_all(schema(kind).as_bytes()))?;
    Ok(EXIT_OK)
}
