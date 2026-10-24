//! Masks URLs and email addresses, and what config `mask` patterns match,
//! such as template syntax (`{{< ref "x" >}}`), so no rule sees them.
//!
//! Patterns run on each block's prose view, after markup is removed and
//! whitespace collapsed. Matched bytes become placeholders, like inline
//! code: a phrase never matches across them.

use std::ops::Range;
use std::sync::LazyLock;

use regex::Regex;

use super::Block;
use super::view::PLACEHOLDER;

/// Config `mask` patterns, compiled.
#[derive(Debug, Default)]
pub struct Mask(Vec<Regex>);

impl Mask {
    /// The error names the first pattern that doesn't compile.
    pub fn try_new(patterns: impl IntoIterator<Item = impl AsRef<str>>) -> Result<Mask, String> {
        patterns
            .into_iter()
            .map(|p| {
                let p = p.as_ref();
                Regex::new(p).map_err(|e| format!("invalid mask `{p}`: {e}"))
            })
            .collect::<Result<_, _>>()
            .map(Mask)
    }

    pub(super) fn apply(&self, block: &mut Block) {
        let ranges: Vec<Range<usize>> = self
            .0
            .iter()
            .flat_map(|re| re.find_iter(&block.text).map(|m| m.range()))
            .collect();
        hide(block, ranges);
    }
}

/// URLs (`https://…`, `www.…`, `mailto:…`) and email addresses. Trailing
/// punctuation isn't part of a URL, nor is a `)` it doesn't open.
static URL: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(
        r"(?:\b[a-zA-Z][a-zA-Z0-9+.-]*://|\bwww\.|\bmailto:)[^\s<>\x1A]+|[\w.+-]+@[\w-]+(?:\.[\w-]+)+",
    )
    .ok()
});

/// Masks the URLs and email addresses in `block`.
pub(super) fn urls(block: &mut Block) {
    let text = &block.text;
    let maybe = ["://", "www.", "mailto:", "@"];
    if !maybe.iter().any(|m| text.contains(m)) {
        return;
    }
    let Some(re) = URL.as_ref() else {
        return;
    };
    let ranges: Vec<Range<usize>> = re
        .find_iter(text)
        .map(|m| m.start()..m.start() + url_len(m.as_str()))
        .collect();
    hide(block, ranges);
}

/// How much of a matched URL is the URL: not trailing punctuation, nor a
/// closing bracket it doesn't open.
fn url_len(url: &str) -> usize {
    let mut end = url.len();
    loop {
        // U+2019 right single quote, U+201D right double quote.
        let trimmed = url[..end].trim_end_matches([
            '.', ',', ':', ';', '!', '?', '\'', '"', '\u{2019}', '\u{201D}',
        ]);
        end = trimmed.len();
        let unbalanced = |open: char, close: char| {
            trimmed.ends_with(close)
                && trimmed.matches(close).count() > trimmed.matches(open).count()
        };
        if unbalanced('(', ')') || unbalanced('[', ']') {
            end -= 1;
        } else {
            return end;
        }
    }
}

/// Turns `ranges` of the view, each over whole characters, into
/// placeholders. One single-byte placeholder per byte keeps the length,
/// char boundaries and the source map valid; a run of them is one hidden
/// thing.
pub(super) fn hide(block: &mut Block, ranges: Vec<Range<usize>>) {
    let placeholder = PLACEHOLDER.to_string();
    for range in ranges {
        let placeholders = placeholder.repeat(range.len());
        block.text.replace_range(range, &placeholders);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::{Format, Options, scan};

    #[test]
    fn masked_text_keeps_its_length_and_map() {
        let mask = Mask::try_new([r"\{\{<.*?>\}\}"]).unwrap();
        let source = "See {{< ref \"naïve\" >}} for more.";
        let options = Options {
            mask: Some(&mask),
            ..Options::default()
        };
        let blocks = scan(Format::Text, source, options).blocks;
        let text = &blocks[0].text;
        assert_eq!(text.len(), source.len());
        assert!(text.starts_with("See \u{1A}"), "{text:?}");
        let end = text.find(" for").unwrap();
        assert_eq!(
            &source[blocks[0].source_range(source, end..text.len())],
            " for more."
        );
    }

    #[test]
    fn urls_and_email_addresses_are_masked() {
        let masked = |source: &str| {
            let blocks = scan(Format::Text, source, Options::default()).blocks;
            blocks[0].text.replace(PLACEHOLDER, "#")
        };
        let hidden = |s: &str| "#".repeat(s.len());
        assert_eq!(
            masked("See https://example.com/perhaps, or www.x.org/a."),
            format!(
                "See {}, or {}.",
                hidden("https://example.com/perhaps"),
                hidden("www.x.org/a")
            )
        );
        let wiki = "https://en.wikipedia.org/wiki/Foo_(bar)";
        assert_eq!(
            masked(&format!("(see {wiki}) now")),
            format!("(see {}) now", hidden(wiki))
        );
        assert_eq!(
            masked("Mail me@example.com today."),
            format!("Mail {} today.", hidden("me@example.com"))
        );
        assert_eq!(masked("Note: it is 5:30."), "Note: it is 5:30.");
    }

    #[test]
    fn invalid_patterns_are_errors() {
        assert!(
            Mask::try_new(["("])
                .unwrap_err()
                .contains("invalid mask `(`")
        );
    }
}
