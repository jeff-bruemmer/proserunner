//! The rule file format, exactly as written on disk. `compile` turns it
//! into something that can match.

#[cfg(test)]
use std::borrow::Cow;
use std::fmt::{self, Display, Formatter};
use std::marker::PhantomData;

#[cfg(test)]
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::Deserialize;
use serde::de::value::MapAccessDeserializer;
use serde::de::{self, Deserializer, MapAccess, Visitor};

pub use super::checks::{
    AcronymsTable, CapitalizationTable, Formula, LimitTable, MetricPer, MetricTable, Per, Style,
    Unit,
};
use crate::scan::Scope;
use crate::settings;

/// The only rule format this version reads.
pub const SCHEMA: u32 = 1;

/// How serious an issue is.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Deserialize, clap::ValueEnum,
)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
#[expect(missing_docs, reason = "variant docs would change --fail-on's help")]
pub enum Level {
    Hint,
    #[default]
    Warning,
    Error,
}

impl Level {
    /// As written in rule files and config.
    pub fn as_str(self) -> &'static str {
        match self {
            Level::Hint => "hint",
            Level::Warning => "warning",
            Level::Error => "error",
        }
    }
}

impl Display for Level {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.pad(self.as_str())
    }
}

/// Whether matching ignores case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
#[expect(clippy::exhaustive_enums, reason = "closed: sensitive or insensitive")]
pub enum Case {
    /// Any case matches, with Unicode case folding.
    #[default]
    Insensitive,
    /// Only the case as written matches.
    Sensitive,
}

impl Case {
    /// As written in rule files.
    pub fn as_str(self) -> &'static str {
        match self {
            Case::Insensitive => "insensitive",
            Case::Sensitive => "sensitive",
        }
    }
}

impl Display for Case {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.pad(self.as_str())
    }
}

/// A proserunner rule file. Its id is `<source>/<file name>`, such as
/// `default/hedging` for the built-in `hedging.toml`.
#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[cfg_attr(test, schemars(title = "proserunner rule"))]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct RuleFile {
    /// The format version: 1.
    #[cfg_attr(test, schemars(extend("const" = 1)))]
    pub schema: u32,
    /// The name people see. By default it comes from the file name:
    /// `needless-variants.toml` is "Needless variants".
    pub name: Option<String>,
    /// The message for each issue. `{match}` stands for the match,
    /// `{suggestion}` for the entry's suggestions and `{rule}` for the
    /// rule's name. An entry's own `message` takes its place.
    pub message: Option<String>,
    /// How serious the rule's issues are. A run fails on issues at the
    /// config's `fail-on` level or above.
    #[serde(default)]
    pub level: Level,
    /// Why the rule exists, for `proserunner explain` and verbose output.
    pub explain: Option<String>,
    /// Whether matching ignores case.
    #[serde(default)]
    pub case: Case,
    /// Where the rule applies. By default, everywhere.
    #[serde(default)]
    pub scope: Vec<Scope>,
    /// The headings the rule applies to, by level: 0 is a document's
    /// title, 1 its outermost sections, 2 the sections inside those. By
    /// default, every level. Other blocks are left to `scope`.
    #[serde(default, rename = "heading-levels")]
    pub heading_levels: Vec<u8>,
    /// Matches never to report, each compared with a whole match using the
    /// rule's case: a string, or `{ pattern = '…' }`, a regex that must
    /// match all of it.
    #[serde(default)]
    pub except: Vec<Except>,
    /// Words and phrases, matched literally. `{a,b}` expands, so
    /// `synerg{y,ies}` is two words. A space matches any run of whitespace,
    /// line breaks included. A word that starts or ends with a letter,
    /// digit or `_` must start or end a word in the text.
    #[serde(default)]
    pub words: Vec<Entry>,
    /// Regular expressions in Rust syntax, used exactly as written.
    /// Lookaround and backreferences work, with a limit on backtracking.
    /// Messages and suggestions can use their groups: `{1}`, `{name}`.
    #[serde(default)]
    pub patterns: Vec<Entry>,
    /// Names written exactly one way, such as "GitHub". Any case matches,
    /// and anything but the term as written is reported.
    #[serde(default)]
    pub terms: Vec<Term>,
    /// Report a word repeated right after itself, such as "the the".
    #[serde(default)]
    pub repeat: bool,
    /// Groups of spellings a document keeps to one of, such as
    /// `[["advisor", "adviser"]]`: the first a document uses wins, and the
    /// others are reported. Braces expand as in `words`, the same way in
    /// each form of a group.
    #[serde(default)]
    pub consistent: Vec<Vec<String>>,
    /// Report sentences or blocks longer than `max` words, sentences or
    /// characters.
    pub limit: Option<LimitTable>,
    /// Report words in a block, usually a heading, that the style
    /// capitalizes otherwise.
    pub capitalization: Option<CapitalizationTable>,
    /// Report an acronym used before it's defined.
    pub acronyms: Option<AcronymsTable>,
    /// Report text that reads above a school grade.
    pub metric: Option<MetricTable>,
    /// Text that `proserunner rules test` checks the rule against.
    #[serde(default)]
    pub examples: Examples,
}

/// Text that `proserunner rules test` checks a rule against.
#[derive(Debug, Clone, Default, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct Examples {
    /// Text the rule must report an issue in.
    #[serde(default)]
    pub bad: Vec<String>,
    /// Text the rule must report nothing in.
    #[serde(default)]
    pub good: Vec<String>,
    /// `[before, after]` pairs: replacing each issue in `before` with its
    /// first suggestion must give `after`.
    #[serde(default)]
    #[cfg_attr(test, schemars(with = "Vec<(String, String)>", extend("default" = [])))]
    pub fix: Vec<Fix>,
}

/// A `fix` pair: text with issues, and the text once each is replaced
/// with its first suggestion.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(from = "(String, String)")]
#[non_exhaustive]
pub struct Fix {
    /// Text with issues.
    pub before: String,
    /// What it must become.
    pub after: String,
}

impl From<(String, String)> for Fix {
    fn from((before, after): (String, String)) -> Self {
        Fix { before, after }
    }
}

/// One `words` or `patterns` entry: a bare string, or a table with
/// `match` and its own `suggest`, `message`, `case` or `level`.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Entry {
    /// The word or pattern: a table's `match`.
    pub pattern: String,
    /// What to write instead, best first.
    pub suggest: Vec<String>,
    /// Its own message, in place of the rule's.
    pub message: Option<String>,
    /// Its own case, in place of the rule's.
    pub case: Option<Case>,
    /// Its own level, in place of the rule's.
    pub level: Option<Level>,
}

/// An entry with settings of its own.
#[derive(Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
struct EntryTable {
    /// The word or pattern.
    #[serde(rename = "match")]
    pattern: String,
    /// What to write instead, best first. Suggestions can use the same
    /// placeholders as messages, except `{suggestion}`.
    #[serde(default)]
    suggest: Vec<String>,
    /// The message for this entry's issues, in place of the rule's.
    message: Option<String>,
    /// Whether this entry ignores case, in place of the rule's `case`.
    case: Option<Case>,
    /// How serious this entry's issues are, in place of the rule's
    /// `level`. The rule's level in config replaces both.
    level: Option<Level>,
}

impl From<String> for Entry {
    fn from(pattern: String) -> Entry {
        Entry {
            pattern,
            suggest: Vec::new(),
            message: None,
            case: None,
            level: None,
        }
    }
}

impl From<EntryTable> for Entry {
    fn from(t: EntryTable) -> Entry {
        Entry {
            pattern: t.pattern,
            suggest: t.suggest,
            message: t.message,
            case: t.case,
            level: t.level,
        }
    }
}

/// One `except` item: a match to drop, written out or as a regex.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Except {
    /// The match, written out.
    Text(String),
    /// A regex that must match the whole match: a table's `pattern`.
    Pattern(String),
}

/// An exception written as a regex.
#[derive(Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
struct ExceptTable {
    /// A regex that must match the whole match, with the rule's case.
    pattern: String,
}

impl From<String> for Except {
    fn from(s: String) -> Except {
        Except::Text(s)
    }
}

impl From<ExceptTable> for Except {
    fn from(t: ExceptTable) -> Except {
        Except::Pattern(t.pattern)
    }
}

/// One `terms` entry: the term as it must be written, and other
/// spellings to find.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Term {
    /// The term, written the one right way.
    pub term: String,
    /// Other spellings to find: a table's `match`.
    pub variants: Vec<String>,
    /// Its own message, in place of the rule's.
    pub message: Option<String>,
    /// Its own level, in place of the rule's.
    pub level: Option<Level>,
}

/// A term with other spellings or settings of its own.
#[derive(Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
struct TermTable {
    /// The term, written the one right way.
    term: String,
    /// Other spellings to find, in any case. Braces expand as in `words`.
    #[serde(default, rename = "match")]
    variants: Vec<String>,
    /// The message for this term's issues, in place of the rule's.
    message: Option<String>,
    /// How serious this term's issues are, in place of the rule's `level`.
    level: Option<Level>,
}

impl From<String> for Term {
    fn from(term: String) -> Term {
        Term {
            term,
            variants: Vec::new(),
            message: None,
            level: None,
        }
    }
}

impl From<TermTable> for Term {
    fn from(t: TermTable) -> Term {
        Term {
            term: t.term,
            variants: t.variants,
            message: t.message,
            level: t.level,
        }
    }
}

/// Reads a bare string, or a table as `Table`, into `T`. Hand-written
/// rather than `#[serde(untagged)]`, which would replace a precise error
/// ("unknown field `sugest`") with "did not match any variant".
struct StringOr<T, Table> {
    expecting: &'static str,
    marker: PhantomData<fn() -> (T, Table)>,
}

fn string_or<'de, D, T, Table>(deserializer: D, expecting: &'static str) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: From<String> + From<Table>,
    Table: Deserialize<'de>,
{
    deserializer.deserialize_any(StringOr::<T, Table> {
        expecting,
        marker: PhantomData,
    })
}

impl<'de, T, Table> Visitor<'de> for StringOr<T, Table>
where
    T: From<String> + From<Table>,
    Table: Deserialize<'de>,
{
    type Value = T;

    fn expecting(&self, f: &mut Formatter) -> fmt::Result {
        f.write_str(self.expecting)
    }

    fn visit_str<E: de::Error>(self, s: &str) -> Result<T, E> {
        Ok(T::from(s.to_owned()))
    }

    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<T, A::Error> {
        Table::deserialize(MapAccessDeserializer::new(map)).map(T::from)
    }
}

impl<'de> Deserialize<'de> for Entry {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        string_or::<D, Entry, EntryTable>(deserializer, "a string, or a table with `match`")
    }
}

impl<'de> Deserialize<'de> for Except {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        string_or::<D, Except, ExceptTable>(deserializer, "a string, or a table with `pattern`")
    }
}

impl<'de> Deserialize<'de> for Term {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        string_or::<D, Term, TermTable>(deserializer, "a string, or a table with `term`")
    }
}

/// The schema of a string-or-table type: a string, or `Table`.
#[cfg(test)]
fn string_or_schema<Table: JsonSchema>(
    generator: &mut SchemaGenerator,
    description: &str,
) -> Schema {
    schemars::json_schema!({
        "description": description,
        "anyOf": [{ "type": "string" }, generator.subschema_for::<Table>()]
    })
}

#[cfg(test)]
impl JsonSchema for Entry {
    fn schema_name() -> Cow<'static, str> {
        "Entry".into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        string_or_schema::<EntryTable>(
            generator,
            "A word or pattern, or a table with `match` and its own `suggest`, `message`, `case` or `level`.",
        )
    }
}

#[cfg(test)]
impl JsonSchema for Except {
    fn schema_name() -> Cow<'static, str> {
        "Except".into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        string_or_schema::<ExceptTable>(
            generator,
            "A match never to report, or a table with a `pattern` that must match all of it.",
        )
    }
}

#[cfg(test)]
impl JsonSchema for Term {
    fn schema_name() -> Cow<'static, str> {
        "Term".into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        string_or_schema::<TermTable>(
            generator,
            "A term as it must be written, or a table with `term` and other spellings to find in `match`.",
        )
    }
}

pub(crate) fn parse(text: &str) -> Result<RuleFile, String> {
    settings::parse(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_are_strings_or_tables() {
        let r = parse(
            r#"
            schema = 1
            message = "m"
            words = ["a", { match = "b", suggest = ["c"] }]
            "#,
        )
        .unwrap();
        assert_eq!(r.words[0].pattern, "a");
        assert_eq!(r.words[1].suggest, vec!["c"]);
    }

    #[test]
    fn unknown_keys_are_errors() {
        let e = parse("schema = 1\nmesage = \"m\"").unwrap_err();
        assert!(e.contains("unknown field `mesage`"), "{e}");
    }

    #[test]
    fn unknown_entry_keys_name_the_key() {
        let e = parse("schema = 1\nwords = [{ match = \"a\", sugest = [] }]").unwrap_err();
        assert!(e.contains("unknown field `sugest`"), "{e}");
    }

    #[test]
    fn excepts_and_terms_are_strings_or_tables() {
        let r = parse(
            r#"
            schema = 1
            except = ["a", { pattern = 'b+' }]
            terms = ["GitHub", { term = "macOS", match = ["mac os"], level = "error" }]
            "#,
        )
        .unwrap();
        assert_eq!(
            r.except,
            [Except::Text("a".into()), Except::Pattern("b+".into())]
        );
        assert_eq!(r.terms[0].term, "GitHub");
        assert_eq!(r.terms[1].variants, ["mac os"]);
        assert_eq!(r.terms[1].level, Some(Level::Error));
        let e = parse("schema = 1\nterms = [{ term = \"a\", suggest = [] }]").unwrap_err();
        assert!(e.contains("unknown field `suggest`"), "{e}");
    }

    #[test]
    fn invalid_scope_is_an_error() {
        let e = parse("schema = 1\nscope = [\"heading\", \"title\"]").unwrap_err();
        assert!(e.contains("title"), "{e}");
    }
}
