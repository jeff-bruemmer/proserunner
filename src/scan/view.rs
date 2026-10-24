//! Builds a block's matching view and its map back to the source.

use std::ops::Range;

use super::{Block, Scope};

/// Stands in for masked inline content such as code spans and HTML. It's
/// neither a word character nor whitespace, so a phrase never matches
/// across it and a word next to it still ends on a boundary.
pub(crate) const PLACEHOLDER: char = '\u{1A}';

pub(super) struct ViewBuilder {
    kind: Scope,
    level: Option<u8>,
    text: String,
    map: Vec<u32>,
    /// Source offset of whitespace seen since the last character.
    pending_space: Option<usize>,
}

impl ViewBuilder {
    /// A block of `kind`; `level` is a heading's level.
    pub(crate) fn new(kind: Scope, level: Option<u8>) -> Self {
        ViewBuilder {
            kind,
            level,
            text: String::new(),
            map: Vec::new(),
            pending_space: None,
        }
    }

    /// Appends source text that starts at source offset `at`, collapsing
    /// whitespace.
    pub(crate) fn push_source(&mut self, text: &str, at: usize) {
        for (i, c) in text.char_indices() {
            if c.is_whitespace() {
                self.space(at + i);
            } else {
                self.flush_space();
                self.push_char(c, at + i, /*contiguous=*/ true);
            }
        }
    }

    /// Appends text that stands for the source bytes `source` without
    /// being them, such as an escape or an entity. Its bytes map to the
    /// start of `source`. When its last character has more than one byte,
    /// the last maps to the end, so a span over the text covers all of
    /// `source`; a one-byte `&` for `&amp;` can only point at its start.
    pub(crate) fn push_rewritten(&mut self, text: &str, source: Range<usize>) {
        for c in text.chars() {
            if c.is_whitespace() {
                self.space(source.start);
            } else {
                self.flush_space();
                self.push_char(c, source.start, /*contiguous=*/ false);
            }
        }
        let last = text.trim_end().chars().next_back();
        if last.is_some_and(|c| c.len_utf8() > 1)
            && source.end > source.start
            && let Some(m) = self.map.last_mut()
        {
            *m = (source.end - 1) as u32;
        }
    }

    /// A line break or other gap that reads as a space.
    pub(crate) fn space(&mut self, at: usize) {
        if self.pending_space.is_none() {
            self.pending_space = Some(at);
        }
    }

    pub(crate) fn placeholder(&mut self, at: usize) {
        self.flush_space();
        self.push_char(PLACEHOLDER, at, /*contiguous=*/ false);
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// The finished block, or `None` if it holds no prose.
    pub(crate) fn finish(self) -> Option<Block> {
        let has_prose = self
            .text
            .chars()
            .any(|c| !c.is_whitespace() && c != PLACEHOLDER);
        has_prose.then_some(Block {
            kind: self.kind,
            level: self.level,
            text: self.text,
            map: self.map,
        })
    }

    fn flush_space(&mut self) {
        if let Some(at) = self.pending_space.take()
            && !self.text.is_empty()
        {
            self.text.push(' ');
            self.map.push(at as u32);
        }
    }

    /// `contiguous`: the char's bytes are the source bytes at `at`.
    fn push_char(&mut self, c: char, at: usize, contiguous: bool) {
        self.text.push(c);
        for b in 0..c.len_utf8() {
            self.map.push((if contiguous { at + b } else { at }) as u32);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapses_whitespace_and_maps_back() {
        let src = "in  order\n   to";
        let mut v = ViewBuilder::new(Scope::Paragraph, None);
        v.push_source(src, 0);
        let b = v.finish().unwrap();
        assert_eq!(b.text, "in order to");
        assert_eq!(&src[b.source_range(src, 0..11)], src);
        assert_eq!(&src[b.source_range(src, 3..8)], "order");
        assert_eq!(&src[b.source_range(src, 9..11)], "to");
    }

    #[test]
    fn multibyte_chars_map_exactly() {
        let src = "naïve café";
        let mut v = ViewBuilder::new(Scope::Paragraph, None);
        v.push_source(src, 0);
        let b = v.finish().unwrap();
        let start = b.text.find("café").unwrap();
        assert_eq!(&src[b.source_range(src, start..b.text.len())], "café");
    }

    #[test]
    fn leading_and_trailing_space_is_dropped() {
        let mut v = ViewBuilder::new(Scope::Paragraph, None);
        v.push_source("  a  ", 0);
        assert_eq!(v.finish().unwrap().text, "a");
    }

    #[test]
    fn placeholder_only_blocks_are_empty() {
        let mut v = ViewBuilder::new(Scope::Paragraph, None);
        v.placeholder(0);
        assert!(v.finish().is_none());
    }
}
