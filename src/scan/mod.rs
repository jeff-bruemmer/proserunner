//! Turns a document into prose blocks.
//!
//! Each block is a *view*: its prose with markup removed, masked content
//! (inline code, HTML, quoted text by default) replaced, and every run of
//! whitespace collapsed to one space. A byte map leads from the view back
//! to the source, so matching never needs to know about markup and every
//! issue still points at exact source bytes.
//!
//! Scanners also collect inline control directives from comments, which
//! `policy` applies after matching.

mod asciidoc;
mod block;
mod control;
mod format;
mod html;
mod markdown;
mod mask;
mod mdx;
mod org;
mod piece;
mod quotes;
mod text;
mod typst;
mod view;

pub use block::{Block, Scanned, Scope};
pub(crate) use control::Control;
pub use control::{ControlError, Directive, Word};
pub use format::{DOCUMENTS, DocumentFormat, Format};
pub use mask::Mask;
pub(crate) use view::PLACEHOLDER;

/// What a scan hides from the rules besides markup. By default, quoted
/// text and code blocks.
// Scanners hold this as `Options<'src>`: both it and the source are shared
// borrows, so one lifetime covers the two.
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct Options<'mask> {
    /// Check quoted text too. By default it's masked.
    pub quoted_text: bool,
    /// Check code blocks too. By default they're skipped.
    pub code_blocks: bool,
    /// Config `mask` patterns.
    pub mask: Option<&'mask Mask>,
}

/// A byte order mark. Scanners never see one; offsets still count it.
pub const BOM: &str = "\u{FEFF}";

/// The prose blocks and directives of `source`.
#[must_use]
pub fn scan(format: Format, source: &str, options: Options<'_>) -> Scanned {
    let body = source.strip_prefix(BOM).unwrap_or(source);
    let mut scanned = format.scan(body, options);
    if body.len() < source.len() {
        scanned.remap(|i| i + BOM.len());
    }
    // Three passes, not one loop doing all three: fused, the lint benches
    // ran 3% slower.
    scanned.blocks.iter_mut().for_each(mask::urls);
    if let Some(mask) = options.mask {
        scanned.blocks.iter_mut().for_each(|b| mask.apply(b));
    }
    if !options.quoted_text {
        scanned.blocks.iter_mut().for_each(quotes::mask);
    }
    scanned
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn texts(blocks: &[Block]) -> Vec<&str> {
        blocks.iter().map(|b| b.text.as_str()).collect()
    }

    pub(crate) fn levels(blocks: &[Block]) -> Vec<(&str, Option<u8>)> {
        blocks.iter().map(|b| (b.text.as_str(), b.level)).collect()
    }

    #[test]
    fn a_byte_order_mark_is_skipped_but_counted() {
        let src = "\u{FEFF}* Head\n\nText.\n";
        let scanned = scan(Format::Org, src, Options::default());
        assert_eq!(texts(&scanned.blocks), ["Head", "Text."]);
        let head = &scanned.blocks[0];
        assert_eq!(head.kind, Scope::Heading);
        assert_eq!(&src[head.source_range(src, 0..4)], "Head");
    }

    #[test]
    fn quoted_text_is_masked_by_default() {
        let blocks = scan(
            Format::Text,
            "She said \"obviously wrong\" and left.",
            Options::default(),
        )
        .blocks;
        let hidden = PLACEHOLDER.to_string().repeat("\"obviously wrong\"".len());
        assert_eq!(texts(&blocks), [format!("She said {hidden} and left.")]);
        let blocks = scan(
            Format::Text,
            "She said \"obviously wrong\" and left.",
            Options {
                quoted_text: true,
                ..Options::default()
            },
        )
        .blocks;
        assert_eq!(texts(&blocks), ["She said \"obviously wrong\" and left."]);
    }
}
