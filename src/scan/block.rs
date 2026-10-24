//! Blocks: the prose of a document, each with its scope and a map from
//! its matching view back to the source.

use std::fmt::{self, Display, Formatter};
use std::ops::Range;

use serde::Deserialize;

use super::Directive;

/// Where in a document a rule applies. A rule with no scope applies to all
/// prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Scope {
    /// Paragraphs, footnotes and captions.
    Paragraph,
    /// Headings and titles.
    Heading,
    /// List items.
    List,
    /// Table cells.
    Table,
    /// Quoted blocks, such as Markdown's `>` lines.
    Blockquote,
    /// Code blocks, when checked (`--code-blocks`).
    Code,
}

impl Scope {
    /// As written in rule files.
    pub fn as_str(self) -> &'static str {
        match self {
            Scope::Paragraph => "paragraph",
            Scope::Heading => "heading",
            Scope::List => "list",
            Scope::Table => "table",
            Scope::Blockquote => "blockquote",
            Scope::Code => "code",
        }
    }
}

impl Display for Scope {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.pad(self.as_str())
    }
}

/// A run of prose: its matching view, and a map from the view back to
/// the source.
#[derive(Debug, Clone)]
pub struct Block {
    pub(crate) kind: Scope,
    pub(crate) level: Option<u8>,
    pub(crate) text: String,
    /// `map[i]` is the source offset of view byte `i`, so it's as long as
    /// `text`.
    pub(super) map: Vec<u32>,
}

impl Block {
    /// The scope it's in.
    pub fn kind(&self) -> Scope {
        self.kind
    }

    /// A heading's level: 0 for a document's title, 1 for its outermost
    /// sections, 2 inside those, and so on. `None` for anything else.
    pub fn level(&self) -> Option<u8> {
        self.level
    }

    /// The matching view.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The source bytes behind view bytes `range`, widened to whole
    /// characters of `source`. A view byte can stand for part of a
    /// character: masking turns a quoted `é` into two placeholders, one
    /// for each of its bytes.
    pub fn source_range(&self, source: &str, range: Range<usize>) -> Range<usize> {
        let start = self.map.get(range.start).copied().unwrap_or(0) as usize;
        let end = match range.end.checked_sub(1).and_then(|i| self.map.get(i)) {
            Some(&last) if range.end > range.start => last as usize + 1,
            _ => start,
        };
        let start = char_start(source, start);
        start..char_end(source, end).max(start)
    }

    /// The source offset of view byte `i`.
    pub fn source_offset(&self, i: usize) -> usize {
        self.map.get(i).copied().unwrap_or(0) as usize
    }
}

/// The start of the character that byte `i` of `s` is in.
fn char_start(s: &str, i: usize) -> usize {
    let mut i = i.min(s.len());
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// The end of the character that byte `i - 1` of `s` is in.
fn char_end(s: &str, i: usize) -> usize {
    let mut i = i.min(s.len());
    while !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// What a scanner finds in a document.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct Scanned {
    /// A block per paragraph, heading, list item, table cell and the like.
    pub blocks: Vec<Block>,
    /// Inline control directives, in source order.
    pub directives: Vec<Directive>,
}

impl Scanned {
    /// Moves every offset through `to`, for a scan of text that was cut
    /// out of the source: `to` turns an offset in that text into one in
    /// the source.
    pub(super) fn remap(&mut self, to: impl Fn(usize) -> usize) {
        for block in &mut self.blocks {
            for m in &mut block.map {
                *m = to(*m as usize) as u32;
            }
        }
        for d in &mut self.directives {
            // `end` is just past the comment: map its last byte.
            d.end = d.end.checked_sub(1).map_or(to(0), |last| to(last) + 1);
            d.action.at = to(d.action.at);
            for id in &mut d.ids {
                id.at = to(id.at);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::scan::{Format, Options, scan};

    #[test]
    fn ranges_cover_whole_characters() {
        // Masking turns each byte of a quoted `é` into a placeholder.
        let src = "He said \"naïve café\" and left.";
        let block = &scan(Format::Text, src, Options::default()).blocks[0];
        for (i, c) in block.text.char_indices() {
            let r = block.source_range(src, i..i + c.len_utf8());
            assert!(src.get(r.clone()).is_some(), "{r:?}");
        }
        let i = src.find('ï').unwrap();
        assert_eq!(block.source_range(src, i..i + 1), i..i + 2);
    }
}
