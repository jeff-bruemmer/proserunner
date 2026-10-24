//! Markdown through `pulldown-cmark`. Only text inside paragraphs,
//! headings, list items, table cells and block quotes is prose. Code,
//! HTML, front matter and link destinations never reach the rules, so
//! there's no need for rules to guess at markup. Inline control
//! directives go in HTML comments.

use pulldown_cmark::{Event, LinkType, Options as ParserOptions, Parser, Tag, TagEnd};

use super::control::{self, Directive};
use super::piece::Piece;
use super::view::ViewBuilder;
use super::{Block, Options, Scanned, Scope};

/// The containers a block sits in, innermost last.
#[derive(Clone, Copy, PartialEq)]
enum Container {
    /// With its level: 1 for `#`.
    Heading(u8),
    TableCell,
    Item,
    BlockQuote,
    Code,
    Other,
}

struct Scanner<'src> {
    source: &'src str,
    options: Options<'src>,
    stack: Vec<Container>,
    /// Inside code (unless checked), HTML blocks or front matter.
    skip: usize,
    /// Inside an autolink, whose text is its URL.
    autolink: bool,
    current: Option<ViewBuilder>,
    blocks: Vec<Block>,
    /// The lines of the HTML block being read, with their source offsets.
    html: Option<Vec<Piece<'src>>>,
    directives: Vec<Directive>,
}

pub(super) fn scan(source: &str, options: Options<'_>) -> Scanned {
    let parser_options = ParserOptions::ENABLE_TABLES
        | ParserOptions::ENABLE_FOOTNOTES
        | ParserOptions::ENABLE_STRIKETHROUGH
        | ParserOptions::ENABLE_TASKLISTS
        | ParserOptions::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | ParserOptions::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS;

    let mut s = Scanner {
        source,
        options,
        stack: Vec::new(),
        skip: 0,
        autolink: false,
        current: None,
        blocks: Vec::new(),
        html: None,
        directives: Vec::new(),
    };
    for (event, range) in Parser::new_ext(source, parser_options).into_offset_iter() {
        s.event(event, range.start, range.end);
    }
    s.close();
    Scanned {
        blocks: s.blocks,
        directives: s.directives,
    }
}

impl<'src> Scanner<'src> {
    fn event(&mut self, event: Event<'_>, start: usize, end: usize) {
        let raw = self.source.get(start..end).unwrap_or("");
        match &event {
            Event::Start(Tag::HtmlBlock) => self.html = Some(Vec::new()),
            Event::Html(_) => {
                if let Some(lines) = self.html.as_mut() {
                    lines.push(Piece {
                        at: start,
                        text: raw,
                    });
                }
            }
            Event::End(TagEnd::HtmlBlock) => {
                if let Some(lines) = self.html.take() {
                    control::html(&lines, &mut self.directives);
                }
            }
            Event::InlineHtml(_) => {
                let piece = Piece {
                    at: start,
                    text: raw,
                };
                control::html(&[piece], &mut self.directives);
            }
            _ => {}
        }
        match event {
            // An autolink's text is its URL: a placeholder, like code.
            Event::Start(Tag::Link {
                link_type: LinkType::Autolink | LinkType::Email,
                ..
            }) if self.skip == 0 => {
                self.view().placeholder(start);
                self.autolink = true;
            }
            Event::End(TagEnd::Link) if self.autolink => self.autolink = false,
            Event::Text(_) if self.autolink => {}
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) if self.skip == 0 => {
                let view = self.view();
                if raw == text.as_ref() {
                    view.push_source(raw, start);
                } else {
                    view.push_rewritten(&text, start..end);
                }
            }
            Event::Code(_)
            | Event::InlineHtml(_)
            | Event::Html(_)
            | Event::InlineMath(_)
            | Event::DisplayMath(_)
            | Event::FootnoteReference(_)
                if self.skip == 0 =>
            {
                self.view().placeholder(start);
            }
            Event::SoftBreak | Event::HardBreak if self.skip == 0 => {
                if let Some(view) = self.current.as_mut() {
                    view.space(start);
                }
            }
            _ => {}
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        let container = match tag {
            Tag::Heading { level, .. } => Container::Heading(level as u8),
            Tag::TableCell => Container::TableCell,
            Tag::Item => Container::Item,
            Tag::BlockQuote(_) => Container::BlockQuote,
            Tag::CodeBlock(_) => Container::Code,
            Tag::HtmlBlock | Tag::MetadataBlock(_) => Container::Other,
            Tag::Paragraph
            | Tag::List(_)
            | Tag::Table(_)
            | Tag::TableHead
            | Tag::TableRow
            | Tag::FootnoteDefinition(_)
            | Tag::DefinitionList
            | Tag::DefinitionListTitle
            | Tag::DefinitionListDefinition => Container::Other,
            // Inline tags: emphasis, links and images don't split prose.
            _ => return,
        };
        self.close();
        let skipped = match tag {
            Tag::CodeBlock(_) => !self.options.code_blocks,
            Tag::HtmlBlock | Tag::MetadataBlock(_) => true,
            _ => false,
        };
        if skipped || self.skip > 0 {
            self.skip += 1;
        }
        self.stack.push(container);
    }

    fn end(&mut self, tag: TagEnd) {
        let block_level = !matches!(
            tag,
            TagEnd::Emphasis
                | TagEnd::Strong
                | TagEnd::Strikethrough
                | TagEnd::Superscript
                | TagEnd::Subscript
                | TagEnd::Link
                | TagEnd::Image
        );
        if !block_level {
            return;
        }
        self.close();
        let was_skipped = self.skip > 0;
        self.stack.pop();
        if was_skipped {
            self.skip -= 1;
        }
    }

    /// The open block, opening one if needed.
    fn view(&mut self) -> &mut ViewBuilder {
        let (kind, level) = self.kind();
        self.current
            .get_or_insert_with(|| ViewBuilder::new(kind, level))
    }

    /// The innermost container's scope, and its level if it's a heading.
    fn kind(&self) -> (Scope, Option<u8>) {
        for c in self.stack.iter().rev() {
            match c {
                Container::Heading(level) => return (Scope::Heading, Some(*level)),
                Container::TableCell => return (Scope::Table, None),
                Container::Item => return (Scope::List, None),
                Container::BlockQuote => return (Scope::Blockquote, None),
                Container::Code => return (Scope::Code, None),
                Container::Other => {}
            }
        }
        (Scope::Paragraph, None)
    }

    fn close(&mut self) {
        if let Some(block) = self.current.take().and_then(ViewBuilder::finish) {
            self.blocks.push(block);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{levels, texts};
    use super::*;

    fn prose(src: &str) -> Vec<String> {
        texts(&scan(src, Options::default()).blocks)
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn markup_is_not_prose() {
        let src = "# A *very* [unique](http://x.com/utilize) title\n\nUse `utilize` here.\n";
        assert_eq!(prose(src), ["A very unique title", "Use \u{1A} here."]);
    }

    #[test]
    fn wrapped_paragraphs_are_one_block() {
        assert_eq!(
            prose("due to the fact\nthat it rained\n"),
            ["due to the fact that it rained"]
        );
    }

    #[test]
    fn block_kinds() {
        let src = "# Title\n\nText.\n\n- item one\n- item two\n\n> quoted\n\n| a | b |\n|---|---|\n| c | d |\n";
        let blocks = scan(src, Options::default()).blocks;
        let kinds: Vec<_> = blocks.iter().map(|b| (b.kind, b.text.as_str())).collect();
        assert_eq!(
            kinds,
            [
                (Scope::Heading, "Title"),
                (Scope::Paragraph, "Text."),
                (Scope::List, "item one"),
                (Scope::List, "item two"),
                (Scope::Blockquote, "quoted"),
                (Scope::Table, "a"),
                (Scope::Table, "b"),
                (Scope::Table, "c"),
                (Scope::Table, "d"),
            ]
        );
    }

    #[test]
    fn heading_levels() {
        let src = "# One\n\n## Two\n\nText.\n\nSetext\n------\n\n> ###### Six\n\n- # In a list\n";
        let blocks = scan(src, Options::default()).blocks;
        assert_eq!(
            levels(&blocks),
            [
                ("One", Some(1)),
                ("Two", Some(2)),
                ("Text.", None),
                ("Setext", Some(2)),
                ("Six", Some(6)),
                ("In a list", Some(1)),
            ]
        );
    }

    #[test]
    fn code_front_matter_and_html_are_skipped() {
        let src = "---\ntitle: utilize\n---\n\nProse.\n\n```\nutilize\n```\n\n    indented code\n\n<div>\nutilize\n</div>\n";
        assert_eq!(prose(src), ["Prose."]);
    }

    #[test]
    fn code_blocks_on_request() {
        let options = Options {
            code_blocks: true,
            ..Options::default()
        };
        let blocks = scan("Prose.\n\n```\nlet x = 1;\n```\n", options).blocks;
        assert_eq!(texts(&blocks), ["Prose.", "let x = 1;"]);
        assert_eq!(blocks[1].kind, Scope::Code);
    }

    #[test]
    fn spans_map_to_source() {
        let src = "Some *very* unique text.\n";
        let blocks = scan(src, Options::default()).blocks;
        let b = &blocks[0];
        let start = b.text.find("very unique").unwrap();
        let range = b.source_range(src, start..start + "very unique".len());
        assert_eq!(&src[range], "very* unique");
    }

    #[test]
    fn nested_list_items() {
        let src = "- outer\n  - inner\n- next\n";
        assert_eq!(prose(src), ["outer", "inner", "next"]);
    }

    #[test]
    fn directives_in_html_comments() {
        let src = "<!-- proserunner-disable a/b -->\n\n> <!--\n> proserunner-enable\n> -->\n\nText <!-- proserunner-disable-next-line --> here.\n\n`<!-- proserunner-disable -->`\n";
        let scanned = scan(src, Options::default());
        let found: Vec<_> = scanned
            .directives
            .iter()
            .map(|d| (d.action.text.as_str(), d.ids.len(), &src[d.end - 3..d.end]))
            .collect();
        assert_eq!(
            found,
            [
                ("disable", 1, "-->"),
                ("enable", 0, "-->"),
                ("disable-next-line", 0, "-->")
            ]
        );
        let id = &scanned.directives[0].ids[0];
        assert_eq!(&src[id.at..id.at + 3], "a/b");
        // The word's offset skips the `> ` of the line it's on.
        let enable = &scanned.directives[1].action;
        assert_eq!(&src[enable.at..enable.at + 18], "proserunner-enable");
    }
}
