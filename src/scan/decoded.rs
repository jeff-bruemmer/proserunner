//! Lines up text a parser decoded with the source it came from, so each
//! byte of a block's view maps to the source bytes it stands for.

use super::html;
use super::view::ViewBuilder;

/// How a string's source writes what it stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Escapes {
    /// Every character is itself: YAML plain and block scalars, and TOML
    /// literal strings.
    Verbatim,
    /// `\` before punctuation, and character references: Markdown text.
    Markdown,
    /// `\n`, `\xHH`, `\uHHHH`, `\UHHHHHHHH` and the like, and `\` before a
    /// line break, which joins the lines: YAML double-quoted strings and
    /// TOML basic strings.
    Backslash,
    /// `''` for `'`: YAML single-quoted strings.
    DoubledQuote,
}

impl Escapes {
    /// The escape at the start of `rest`, the source, for the start of
    /// `want`, the decoded text: how many bytes of each it covers.
    fn at(self, rest: &str, want: &str) -> Option<(usize, usize)> {
        let first = want.chars().next()?.len_utf8();
        let (from, to) = match self {
            Escapes::Verbatim => return None,
            Escapes::Markdown => match rest.as_bytes().first()? {
                b'\\' if rest[1..].starts_with(&want[..first]) => (1 + first, first),
                b'&' => html::reference(rest)
                    .filter(|(_, decoded)| want.starts_with(decoded.as_str()))
                    .map(|(len, decoded)| (len, decoded.len()))?,
                _ => return None,
            },
            Escapes::Backslash => {
                let after = rest.strip_prefix('\\')?;
                let gap = blanks(after);
                if after[..gap].contains(['\r', '\n']) {
                    (1 + gap, whitespace(want))
                } else {
                    let len = match after.chars().next()? {
                        'x' => 3,
                        'u' => 5,
                        'U' => 9,
                        c => c.len_utf8(),
                    };
                    (1 + len, first)
                }
            }
            Escapes::DoubledQuote => {
                (rest.starts_with("''") && want.starts_with('\'')).then_some((2, 1))?
            }
        };
        (rest.get(..from).is_some() && want.get(..to).is_some()).then_some((from, to))
    }

    /// The bytes an escape can start with.
    fn starts(self) -> &'static [u8] {
        match self {
            Escapes::Verbatim => b"",
            Escapes::Markdown => b"\\&",
            Escapes::Backslash => b"\\",
            Escapes::DoubledQuote => b"'",
        }
    }
}

/// Appends `value`, which a parser decoded from `raw`, the source at
/// offset `at`. What reads as the source does is source text, and an
/// escape maps to its source bytes. Once they stop lining up, the rest
/// maps to the rest of `raw`.
pub(super) fn push(view: &mut ViewBuilder, value: &str, raw: &str, at: usize, escapes: Escapes) {
    let (mut i, mut j) = (0, 0);
    while j < value.len() {
        let (rest, want) = (&raw[i..], &value[j..]);
        if let Some((from, to)) = escapes.at(rest, want) {
            view.push_rewritten(&want[..to], at + i..at + i + from);
            (i, j) = (i + from, j + to);
            continue;
        }
        let run = same(rest, want, escapes);
        if run > 0 {
            view.push_source(&rest[..run], at + i);
            (i, j) = (i + run, j + run);
            continue;
        }
        let (gap, space) = (whitespace(rest), whitespace(want));
        if gap + space > 0 {
            view.space(at + i);
            (i, j) = (i + gap, j + space);
            continue;
        }
        view.push_rewritten(want, at + i..at + raw.len());
        return;
    }
}

/// How many bytes `rest` and `want` share at their start, up to the next
/// byte that can start an escape, and ending on a character boundary.
fn same(rest: &str, want: &str, escapes: Escapes) -> usize {
    let starts = escapes.starts();
    let run = rest
        .bytes()
        .zip(want.bytes())
        .enumerate()
        .take_while(|&(k, (a, b))| a == b && (k == 0 || !starts.contains(&a)))
        .count();
    (0..=run)
        .rev()
        .find(|&k| rest.is_char_boundary(k))
        .unwrap_or(0)
}

/// The length of the whitespace `s` starts with.
fn whitespace(s: &str) -> usize {
    s.len() - s.trim_start().len()
}

/// The length of the spaces, tabs and line breaks `s` starts with.
fn blanks(s: &str) -> usize {
    s.len() - s.trim_start_matches([' ', '\t', '\r', '\n']).len()
}

#[cfg(test)]
mod tests {
    use super::super::{Block, Scope};
    use super::*;

    /// `value` decoded from `raw`, which starts at offset 0.
    fn block(value: &str, raw: &str, escapes: Escapes) -> Block {
        let mut view = ViewBuilder::new(Scope::Paragraph, None);
        push(&mut view, value, raw, 0, escapes);
        view.finish().unwrap()
    }

    /// The source behind the view's `word`.
    fn source_of<'a>(b: &Block, raw: &'a str, word: &str) -> &'a str {
        let i = b.text.find(word).unwrap();
        &raw[b.source_range(raw, i..i + word.len())]
    }

    /// The source from where the view's `word` starts. A one-byte
    /// character for an escape maps to the escape's first byte.
    fn source_from<'a>(b: &Block, raw: &'a str, word: &str) -> &'a str {
        let i = b.text.find(word).unwrap();
        &raw[b.source_offset(i)..]
    }

    #[test]
    fn markdown_escapes_and_references() {
        let raw = r"caf&eacute; \*not \\ &amp;amp; very unique";
        let b = block(r"café *not \ &amp; very unique", raw, Escapes::Markdown);
        assert_eq!(b.text, r"café *not \ &amp; very unique");
        assert_eq!(source_of(&b, raw, "é"), "&eacute;");
        assert!(source_from(&b, raw, "*").starts_with(r"\*not"));
        assert!(source_from(&b, raw, r"\").starts_with(r"\\ &"));
        assert_eq!(source_of(&b, raw, "&amp;"), "&amp;amp;");
        assert_eq!(source_of(&b, raw, "very unique"), "very unique");
    }

    #[test]
    fn a_literal_ampersand_or_backslash_is_text() {
        let raw = r"AT&T and C:\dir here";
        let b = block(raw, raw, Escapes::Markdown);
        assert_eq!(source_of(&b, raw, "here"), "here");
    }

    #[test]
    fn backslash_escapes_cover_their_source() {
        let raw = r#"\"Caf\u00E9\" \x41 \U0001F600\tand\_very unique"#;
        let b = block(
            "\"Café\" A 😀\tand\u{A0}very unique",
            raw,
            Escapes::Backslash,
        );
        assert_eq!(b.text, "\"Café\" A 😀 and very unique");
        assert!(source_from(&b, raw, "\"").starts_with(r#"\"Caf"#));
        assert_eq!(source_of(&b, raw, "é"), r"\u00E9");
        assert!(source_from(&b, raw, "A").starts_with(r"\x41"));
        assert_eq!(source_of(&b, raw, "😀"), r"\U0001F600");
        assert_eq!(source_of(&b, raw, "very unique"), "very unique");
    }

    #[test]
    fn a_backslash_before_a_line_break_joins_the_lines() {
        let raw = "ab\\  \n    cd unique";
        let b = block("abcd unique", raw, Escapes::Backslash);
        assert_eq!(b.text, "abcd unique");
        assert_eq!(source_of(&b, raw, "cd"), "cd");
        assert_eq!(source_of(&b, raw, "unique"), "unique");
    }

    #[test]
    fn doubled_quotes() {
        let raw = "it''s very unique";
        let b = block("it's very unique", raw, Escapes::DoubledQuote);
        assert!(source_from(&b, raw, "'").starts_with("''s"));
        assert_eq!(source_of(&b, raw, "very unique"), "very unique");
    }

    #[test]
    fn whitespace_on_one_side() {
        // Folded YAML: the source's line break and indentation are one
        // space, and a kept final line break has no source left.
        let raw = "very\n  unique";
        let b = block("very unique\n", raw, Escapes::Verbatim);
        assert_eq!(b.text, "very unique");
        assert_eq!(source_of(&b, raw, "unique"), "unique");
    }

    #[test]
    fn text_that_does_not_line_up_maps_to_the_rest() {
        for (value, raw) in [("unrelated", "text"), ("longer than", "it"), ("é", "e")] {
            let b = block(value, raw, Escapes::Backslash);
            assert_eq!(b.text, value);
            for (i, c) in b.text.char_indices() {
                let r = b.source_range(raw, i..i + c.len_utf8());
                assert!(raw.get(r.clone()).is_some(), "{r:?}");
            }
        }
    }
}
