//! Front matter: YAML between `---` lines, or TOML between `+++` lines, at
//! the very start of a Markdown or MDX file, read with `saphyr-parser` and
//! `toml`. The strings under the keys `Options::front_matter` names are
//! prose: `title` and `subtitle` are the document's title, and the rest
//! are paragraphs. Keys, other values and comments aren't prose, and
//! front matter that doesn't parse isn't read.

use std::borrow::Cow;
use std::ops::Range;

use saphyr_parser::{Event, Parser, ScalarStyle, Span};
use toml::Spanned;
use toml::de::{DeTable, DeValue};

use super::decoded::{self, Escapes};
use super::view::ViewBuilder;
use super::{Block, Scope};

/// What front matter is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Syntax {
    Yaml,
    Toml,
}

/// The prose of front matter `text`, which starts at source offset `at`:
/// a block per string under one of `keys`, in source order.
pub(super) fn blocks(syntax: Syntax, text: &str, at: usize, keys: &[&str]) -> Vec<Block> {
    if keys.is_empty() {
        return Vec::new();
    }
    let mut found = Found {
        text,
        at,
        keys,
        blocks: Vec::new(),
    };
    let parsed = match syntax {
        Syntax::Yaml => yaml(&mut found),
        Syntax::Toml => toml(&mut found),
    };
    parsed.map(|()| found.blocks).unwrap_or_default()
}

/// One front matter's text, and the blocks found in it.
struct Found<'a> {
    text: &'a str,
    at: usize,
    keys: &'a [&'a str],
    blocks: Vec<Block>,
}

impl Found<'_> {
    /// The string at `path`, `value`, which was decoded from `text[raw]`,
    /// if one of the keys selects it.
    fn string(&mut self, path: &[&str], value: &str, raw: Range<usize>, escapes: Escapes) {
        if !selected(self.keys, path) {
            return;
        }
        let Some(source) = self.text.get(raw.clone()) else {
            return;
        };
        let mut view = if is_title(path) {
            ViewBuilder::new(Scope::Heading, Some(0))
        } else {
            ViewBuilder::new(Scope::Paragraph, None)
        };
        decoded::push(&mut view, value, source, self.at + raw.start, escapes);
        self.blocks.extend(view.finish());
    }
}

/// Whether one of `keys` is where `path` starts.
fn selected(keys: &[&str], path: &[&str]) -> bool {
    keys.iter().any(|key| {
        key.split('.').count() <= path.len() && key.split('.').zip(path).all(|(k, p)| k == *p)
    })
}

/// Whether a string at `path` is part of the document's title.
fn is_title(path: &[&str]) -> bool {
    matches!(path.first(), Some(&("title" | "subtitle")))
}

/// A YAML collection the walk is inside.
enum Frame<'a> {
    Mapping {
        /// The key of the entry being read, if it's a string.
        key: Option<Cow<'a, str>>,
        /// Whether the next node is a key rather than a value.
        at_key: bool,
    },
    Sequence,
}

/// The strings of YAML `found.text`, or `None` if it isn't YAML.
fn yaml(found: &mut Found<'_>) -> Option<()> {
    let starts = char_starts(found.text);
    let mut stack = Vec::new();
    for event in Parser::new_from_str(found.text) {
        let (event, span) = event.ok()?;
        match event {
            Event::Scalar(value, style, ..) => {
                let (open, escapes) = yaml_opening(style);
                if let (Some(path), Some(raw)) = (path(&stack), raw(&starts, span, open)) {
                    found.string(&path, &value, raw, escapes);
                }
                ended(&mut stack, Some(value));
            }
            Event::MappingStart(..) => stack.push(Frame::Mapping {
                key: None,
                at_key: true,
            }),
            Event::SequenceStart(..) => stack.push(Frame::Sequence),
            Event::MappingEnd | Event::SequenceEnd => {
                stack.pop();
                ended(&mut stack, None);
            }
            Event::Alias(_) => ended(&mut stack, None),
            _ => {}
        }
    }
    Some(())
}

/// The keys leading to the node being read, or `None` if it's a key, is
/// inside one, or is under a key that isn't a string.
fn path<'s>(stack: &'s [Frame<'_>]) -> Option<Vec<&'s str>> {
    stack
        .iter()
        .filter_map(|frame| match frame {
            Frame::Mapping { key, at_key } => Some(key.as_deref().filter(|_| !at_key)),
            Frame::Sequence => None,
        })
        .collect()
}

/// A node ended: `scalar`'s value if it was a scalar.
fn ended<'a>(stack: &mut [Frame<'a>], scalar: Option<Cow<'a, str>>) {
    if let Some(Frame::Mapping { key, at_key }) = stack.last_mut() {
        if *at_key {
            *key = scalar;
        }
        *at_key = !*at_key;
    }
}

/// How long a scalar's opening delimiter is, and how its text is escaped.
/// A block scalar's span starts after its header.
fn yaml_opening(style: ScalarStyle) -> (usize, Escapes) {
    match style {
        ScalarStyle::Plain | ScalarStyle::Literal | ScalarStyle::Folded => (0, Escapes::Verbatim),
        ScalarStyle::SingleQuoted => (1, Escapes::DoubledQuote),
        ScalarStyle::DoubleQuoted => (1, Escapes::Backslash),
    }
}

/// The bytes of `span` past its first `open`, through `starts`.
fn raw(starts: &[usize], span: Span, open: usize) -> Option<Range<usize>> {
    let start = starts.get(span.start.index())? + open;
    let end = *starts.get(span.end.index())?;
    Some(start..end)
}

/// The byte offset of each character of `text`, then `text`'s length.
/// `saphyr-parser` counts characters, though its docs say bytes.
fn char_starts(text: &str) -> Vec<usize> {
    text.char_indices()
        .map(|(i, _)| i)
        .chain([text.len()])
        .collect()
}

/// The strings of TOML `found.text`, or `None` if it isn't TOML.
fn toml(found: &mut Found<'_>) -> Option<()> {
    let table = DeTable::parse(found.text).ok()?;
    toml_table(found, &mut Vec::new(), table.get_ref());
    // A table's entries are in the order it was made in: `[a.b]` after
    // `[c]` joins `a`'s.
    found.blocks.sort_by_key(|b| b.source_offset(0));
    Some(())
}

/// The strings in `table`, at `path`.
fn toml_table<'t>(found: &mut Found<'_>, path: &mut Vec<&'t str>, table: &'t DeTable<'_>) {
    for (key, value) in table {
        path.push(key.get_ref().as_ref());
        toml_value(found, path, value);
        path.pop();
    }
}

/// The strings in `value`, at `path`.
fn toml_value<'t>(found: &mut Found<'_>, path: &mut Vec<&'t str>, value: &'t Spanned<DeValue<'_>>) {
    match value.get_ref() {
        DeValue::String(text) => {
            let raw = value.span();
            let (open, escapes) = toml_opening(found.text.get(raw.clone()).unwrap_or(""));
            found.string(path, text, raw.start + open..raw.end, escapes);
        }
        DeValue::Array(items) => {
            for item in items {
                toml_value(found, path, item);
            }
        }
        DeValue::Table(table) => toml_table(found, path, table),
        _ => {}
    }
}

/// How long a string's opening quotes are, and how its text is escaped.
fn toml_opening(raw: &str) -> (usize, Escapes) {
    if raw.starts_with("\"\"\"") {
        (3, Escapes::Backslash)
    } else if raw.starts_with("'''") {
        (3, Escapes::Verbatim)
    } else if raw.starts_with('"') {
        (1, Escapes::Backslash)
    } else {
        (1, Escapes::Verbatim)
    }
}

#[cfg(test)]
mod tests {
    use super::super::FRONT_MATTER;
    use super::*;

    const AT: usize = 4;

    /// The blocks of front matter `text`, which starts at `AT`, with the
    /// default keys: each block's text, scope and level.
    fn read(syntax: Syntax, text: &str) -> Vec<(String, Scope, Option<u8>)> {
        blocks(syntax, text, AT, FRONT_MATTER)
            .into_iter()
            .map(|b| (b.text, b.kind, b.level))
            .collect()
    }

    fn texts(syntax: Syntax, text: &str, keys: &[&str]) -> Vec<String> {
        blocks(syntax, text, AT, keys)
            .into_iter()
            .map(|b| b.text)
            .collect()
    }

    /// The source behind `word` in the block that has it, from `text`,
    /// which starts at `AT`.
    fn source_of<'a>(syntax: Syntax, text: &'a str, word: &str) -> &'a str {
        let source = format!("{}{text}", " ".repeat(AT));
        let found = blocks(syntax, text, AT, FRONT_MATTER);
        let b = found.iter().find(|b| b.text.contains(word)).unwrap();
        let i = b.text.find(word).unwrap();
        let r = b.source_range(&source, i..i + word.len());
        &text[r.start - AT..r.end - AT]
    }

    #[test]
    fn default_keys_and_scopes() {
        let text = "title: A title\nsubtitle: Its subtitle\nauthor: Ann Author\ndescription: What it says.\nsummary: In short.\nabstract: In brief.\nexcerpt: A part.\ntags: [utilize]\nlayout: utilize\n";
        let heading = |t: &str| (t.to_owned(), Scope::Heading, Some(0));
        let paragraph = |t: &str| (t.to_owned(), Scope::Paragraph, None);
        assert_eq!(
            read(Syntax::Yaml, text),
            [
                heading("A title"),
                heading("Its subtitle"),
                paragraph("What it says."),
                paragraph("In short."),
                paragraph("In brief."),
                paragraph("A part."),
            ]
        );
        let toml = "title = 'A title'\nauthor = 'Ann Author'\ndescription = 'What it says.'\ntags = ['utilize']\n";
        assert_eq!(
            read(Syntax::Toml, toml),
            [heading("A title"), paragraph("What it says.")]
        );
    }

    #[test]
    fn every_string_under_a_key() {
        let text = "title:\n  en: The title\n  fr: Le titre\ndescription:\n  - One line.\n  - Another line.\nsummary: 2024\n";
        assert_eq!(
            texts(Syntax::Yaml, text, FRONT_MATTER),
            [
                "The title",
                "Le titre",
                "One line.",
                "Another line.",
                "2024"
            ]
        );
        let toml = "description = ['One line.', 'Another line.']\nsummary = 2024\n[title]\nen = 'The title'\n";
        assert_eq!(
            texts(Syntax::Toml, toml, FRONT_MATTER),
            ["One line.", "Another line.", "The title"]
        );
    }

    #[test]
    fn keys_are_not_prose() {
        let text = "description:\n  utilize: x\n";
        assert_eq!(texts(Syntax::Yaml, text, FRONT_MATTER), ["x"]);
        let text = "? [description, utilize]\n: x\ndescription: &a y\nsummary: *a\n";
        assert_eq!(texts(Syntax::Yaml, text, FRONT_MATTER), ["y"]);
    }

    #[test]
    fn yaml_styles_map_to_source() {
        let cases = [
            "description: a plain\n  very unique line\n",
            "description: 'it''s very unique'\n",
            "description: \"\\\"Caf\\u00E9\\\" \\\n   very unique\"  # a comment\n",
            "description: |\n  A literal\n    very unique\n",
            "description: >-\n  Folded\n  very unique\n",
            "description: |2\n     indented first\n   very unique\n",
            "description: a plain\r\n  very unique line\r\n",
        ];
        for text in cases {
            assert_eq!(
                source_of(Syntax::Yaml, text, "very unique"),
                "very unique",
                "{text:?}"
            );
        }
        let text = "description: \"Caf\\u00E9 utilize\"\n";
        assert_eq!(source_of(Syntax::Yaml, text, "é"), "\\u00E9");
    }

    #[test]
    fn yaml_positions_count_characters() {
        // saphyr-parser's markers count characters. If they ever count
        // bytes, as its docs say, the span lands two bytes late.
        let text = "author: Zoë Ångström\ntitle: Café utilize\n";
        assert_eq!(source_of(Syntax::Yaml, text, "utilize"), "utilize");
    }

    #[test]
    fn toml_strings_map_to_source() {
        let cases = [
            "description = \"\\\"Caf\\u00E9\\\" very unique\"\n",
            "description = 'C:\\dir very unique'\n",
            "description = \"\"\"\nA line \\\n   very unique\"\"\"\n",
            "description = '''\nA line\nvery unique'''\n",
        ];
        for text in cases {
            assert_eq!(
                source_of(Syntax::Toml, text, "very unique"),
                "very unique",
                "{text:?}"
            );
        }
    }

    #[test]
    fn dotted_paths() {
        let keys = ["params.summary"];
        let yaml = "summary: top\nparams:\n  summary: nested\n";
        assert_eq!(texts(Syntax::Yaml, yaml, &keys), ["nested"]);
        let toml = "summary = 'top'\nparams.summary = 'dotted'\n";
        assert_eq!(texts(Syntax::Toml, toml, &keys), ["dotted"]);
        let toml = "summary = 'top'\n[params]\nsummary = 'table'\n";
        assert_eq!(texts(Syntax::Toml, toml, &keys), ["table"]);
        let yaml = "faq:\n  - question: Why?\n    answer: Because.\n  - answer: Still.\n";
        assert_eq!(
            texts(Syntax::Yaml, yaml, &["faq.answer"]),
            ["Because.", "Still."]
        );
    }

    #[test]
    fn toml_blocks_are_in_source_order() {
        let toml =
            "[a]\ndescription = 'one'\n[b]\ndescription = 'two'\n[a.c]\ndescription = 'three'\n";
        let keys = ["a", "b"];
        assert_eq!(texts(Syntax::Toml, toml, &keys), ["one", "two", "three"]);
    }

    #[test]
    fn invalid_front_matter_is_not_read() {
        for text in [
            "title: 'unclosed\n",
            "title: a\n\tdescription: b\n",
            "title: [a\n",
        ] {
            assert!(
                texts(Syntax::Yaml, text, FRONT_MATTER).is_empty(),
                "{text:?}"
            );
        }
        for text in ["title =\n", "title = 'a'\ntitle = 'b'\n"] {
            assert!(
                texts(Syntax::Toml, text, FRONT_MATTER).is_empty(),
                "{text:?}"
            );
        }
    }

    #[test]
    fn no_keys_read_nothing() {
        assert!(texts(Syntax::Yaml, "title: A title\n", &[]).is_empty());
    }
}
