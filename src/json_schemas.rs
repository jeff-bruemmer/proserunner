//! The JSON Schemas in `schemas/`, for editors that check and complete
//! TOML (taplo, Even Better TOML). They're generated from the types that
//! read each file, so they can't drift from what proserunner accepts; this
//! test fails when the files are out of date, and
//! `PROSERUNNER_WRITE_SCHEMAS=1 cargo test json_schemas` rewrites them.

use std::env;
use std::fs;
use std::path::Path;

use schemars::JsonSchema;
use schemars::generate::SchemaSettings;
use serde_json::Value;

use crate::config::ConfigFile;
use crate::ignores::IgnoreFile;
use crate::rules::RuleFile;

/// Draft 7, which every TOML editor reads.
fn generate<T: JsonSchema>() -> String {
    let schema = SchemaSettings::draft07()
        .into_generator()
        .into_root_schema_for::<T>();
    let mut value = schema.to_value();
    tidy(&mut value);
    serde_json::to_string_pretty(&value).unwrap_or_default() + "\n"
}

/// Drops `null`, which TOML doesn't have, so an optional key is just
/// optional, and unwraps descriptions, which come from wrapped doc
/// comments.
fn tidy(value: &mut Value) {
    match value {
        Value::Object(map) => {
            if let Some(Value::String(text)) = map.get_mut("description") {
                let paragraphs: Vec<String> =
                    text.split("\n\n").map(|p| p.replace('\n', " ")).collect();
                *text = paragraphs.join("\n\n");
            }
            if let Some(Value::Array(types)) = map.get_mut("type") {
                types.retain(|t| t != "null");
                if let [only] = types.as_slice() {
                    let only = only.clone();
                    map.insert("type".into(), only);
                }
            }
            if let Some(Value::Array(any)) = map.get_mut("anyOf") {
                any.retain(|s| s.get("type").is_none_or(|t| t != "null"));
                // One schema left, kept beside the description: draft 7
                // ignores keys next to a `$ref`, but not next to `allOf`.
                if any.len() == 1
                    && let Some(any) = map.remove("anyOf")
                {
                    map.insert("allOf".into(), any);
                }
            }
            map.values_mut().for_each(tidy);
        }
        Value::Array(items) => items.iter_mut().for_each(tidy),
        _ => {}
    }
}

#[test]
fn schemas_are_current() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("schemas");
    let write = env::var_os("PROSERUNNER_WRITE_SCHEMAS").is_some();
    let mut stale = Vec::new();
    for (name, text) in [
        ("config", generate::<ConfigFile>()),
        ("rule", generate::<RuleFile>()),
        ("ignores", generate::<IgnoreFile>()),
    ] {
        let path = dir.join(format!("{name}.schema.json"));
        if write {
            fs::create_dir_all(&dir).unwrap();
            fs::write(&path, &text).unwrap();
        } else if fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
            stale.push(path.display().to_string());
        }
    }
    assert!(
        stale.is_empty(),
        "out of date: {stale:?}; PROSERUNNER_WRITE_SCHEMAS=1 cargo test json_schemas rewrites them"
    );
}
