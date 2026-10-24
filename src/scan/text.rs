//! Plain text: paragraphs separated by blank lines. Lines between ```
//! fences are code. An inline control directive is a line of its own.

use super::control;
use super::view::ViewBuilder;
use super::{Block, Options, Scanned, Scope};

pub(super) fn scan(source: &str, options: Options<'_>) -> Scanned {
    let mut blocks = Vec::new();
    let mut directives = Vec::new();
    let mut current: Option<ViewBuilder> = None;
    let mut in_code = false;
    let mut offset = 0;

    fn finish(view: Option<ViewBuilder>, blocks: &mut Vec<Block>) {
        if let Some(block) = view.and_then(ViewBuilder::finish) {
            blocks.push(block);
        }
    }

    for line in source.split_inclusive('\n') {
        let at = offset;
        offset += line.len();
        let content = line.trim_end_matches(['\n', '\r']);

        if content.trim_start().starts_with("```") {
            finish(current.take(), &mut blocks);
            in_code = !in_code;
            continue;
        }
        if in_code && !options.code_blocks {
            continue;
        }
        if content.trim().is_empty() {
            finish(current.take(), &mut blocks);
            continue;
        }
        if !in_code && let Some(d) = control::parse(content, at + content.len(), |i| at + i) {
            finish(current.take(), &mut blocks);
            directives.push(d);
            continue;
        }

        let kind = if in_code {
            Scope::Code
        } else {
            Scope::Paragraph
        };
        let view = current.get_or_insert_with(|| ViewBuilder::new(kind, None));
        if !view.is_empty() {
            view.space(at.saturating_sub(1));
        }
        view.push_source(content, at);
    }
    finish(current.take(), &mut blocks);
    Scanned { blocks, directives }
}

#[cfg(test)]
mod tests {
    use super::super::tests::texts;
    use super::*;

    #[test]
    fn paragraphs_join_wrapped_lines() {
        let src = "due to the fact\nthat it rained.\n\nNext paragraph.\n";
        let blocks = scan(src, Options::default()).blocks;
        assert_eq!(
            texts(&blocks),
            ["due to the fact that it rained.", "Next paragraph."]
        );
    }

    #[test]
    fn fenced_code_is_skipped_unless_asked() {
        let src = "Before.\n```\nlet x = 1;\n```\nAfter.\n";
        assert_eq!(
            texts(&scan(src, Options::default()).blocks),
            ["Before.", "After."]
        );
        let with_code = Options {
            code_blocks: true,
            ..Options::default()
        };
        let blocks = scan(src, with_code).blocks;
        assert_eq!(texts(&blocks), ["Before.", "let x = 1;", "After."]);
        assert_eq!(blocks[1].kind, Scope::Code);
    }

    #[test]
    fn crlf_lines() {
        let blocks = scan("one\r\ntwo\r\n", Options::default()).blocks;
        assert_eq!(texts(&blocks), ["one two"]);
    }

    #[test]
    fn directives_are_lines_of_their_own() {
        let src =
            "One.\n  proserunner-disable-next-line a/b\nTwo.\n```\nproserunner-disable\n```\n";
        let scanned = scan(src, Options::default());
        assert_eq!(texts(&scanned.blocks), ["One.", "Two."]);
        assert_eq!(scanned.directives.len(), 1);
        let d = &scanned.directives[0];
        assert_eq!(d.action.text, "disable-next-line");
        assert_eq!(&src[d.ids[0].at..d.end], "a/b");
    }
}
