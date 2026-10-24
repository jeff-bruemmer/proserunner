//! What every TOML file proserunner reads has in common: one error message
//! per parse failure, and a `schema` version to check.

use std::fmt::Write as _;

use serde::de::DeserializeOwned;

/// `text` as `T`, or the TOML error as one message (it gives the line and
/// column, and shows the line).
pub(crate) fn parse<T: DeserializeOwned>(text: &str) -> Result<T, String> {
    toml::from_str(text).map_err(|e| e.to_string().trim_end().to_owned())
}

/// An error unless a file's `schema` is the one this version reads.
pub(crate) fn check_schema(found: u32, reads: u32) -> Result<(), String> {
    if found == reads {
        Ok(())
    } else {
        Err(format!(
            "schema {found} isn't supported; this version of proserunner reads schema {reads}"
        ))
    }
}

/// A TOML string: literal (`'...'`) when that avoids escaping backslashes,
/// basic otherwise.
pub(crate) fn quote(s: &str) -> String {
    if s.contains('\\') && !s.contains('\'') && !s.chars().any(char::is_control) {
        return format!("'{s}'");
    }
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{:04X}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_versions() {
        assert!(check_schema(1, 1).is_ok());
        assert_eq!(
            check_schema(2, 1).unwrap_err(),
            "schema 2 isn't supported; this version of proserunner reads schema 1"
        );
    }
}
