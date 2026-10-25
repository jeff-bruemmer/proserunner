//! Markdown through `pulldown-cmark`. Only text inside paragraphs,
//! headings, list items, table cells and block quotes is prose, and the
//! strings `front_matter` reads. Code, HTML, the rest of front matter and
//! link destinations never reach the rules, so there's no need for rules
//! to guess at markup. Inline control directives go in HTML comments.

use std::iter::Peekable;
use std::ops::Range;

use pulldown_cmark::{
    Event, LinkType, MetadataBlockKind, Options as ParserOptions, Parser, Tag, TagEnd,
};

use super::control::{self, Directive};
use super::front_matter::{self, Syntax};
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
    /// Inside code (unless checked) or HTML blocks.
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
    let mut events = Parser::new_ext(source, extensions() | metadata(source))
        .into_offset_iter()
        .peekable();
    let (mut blocks, body) = take_front_matter(&mut events, source, options.front_matter);
    let rest = read(source, options, events).unwrap_or_else(|| {
        // pulldown-cmark reads a metadata block wherever a block starts,
        // but only one at the very start is front matter. Read the body
        // again without them.
        let text = source.get(body..).unwrap_or("");
        let events = Parser::new_ext(text, extensions()).into_offset_iter();
        let mut rest = read(text, options, events).unwrap_or_default();
        rest.remap(|i| i + body);
        rest
    });
    blocks.extend(rest.blocks);
    Scanned {
        blocks,
        directives: rest.directives,
    }
}

fn extensions() -> ParserOptions {
    ParserOptions::ENABLE_TABLES
        | ParserOptions::ENABLE_FOOTNOTES
        | ParserOptions::ENABLE_STRIKETHROUGH
        | ParserOptions::ENABLE_TASKLISTS
}

/// The metadata blocks to read: only the kind `source` starts with, if
/// any, since only front matter is one.
fn metadata(source: &str) -> ParserOptions {
    if source.starts_with("---") {
        ParserOptions::ENABLE_YAML_STYLE_METADATA_BLOCKS
    } else if source.starts_with("+++") {
        ParserOptions::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS
    } else {
        ParserOptions::empty()
    }
}

/// The blocks of the front matter `events` start with, if they do, and
/// where the body after it starts.
fn take_front_matter<'a>(
    events: &mut Peekable<impl Iterator<Item = (Event<'a>, Range<usize>)>>,
    source: &str,
    keys: &[&str],
) -> (Vec<Block>, usize) {
    let syntax = match events.peek() {
        Some((Event::Start(Tag::MetadataBlock(kind)), range)) if range.start == 0 => match kind {
            MetadataBlockKind::YamlStyle => Syntax::Yaml,
            MetadataBlockKind::PlusesStyle => Syntax::Toml,
        },
        _ => return (Vec::new(), 0),
    };
    // Its text comes in runs that end before each `\r`.
    let mut text: Option<Range<usize>> = None;
    for (event, range) in events.by_ref() {
        match event {
            Event::Text(_) => text = Some(text.map_or(range.start, |t| t.start)..range.end),
            Event::End(TagEnd::MetadataBlock(_)) => {
                let blocks = text
                    .and_then(|t| {
                        let text = source.get(t.clone())?;
                        Some(front_matter::blocks(syntax, text, t.start, keys))
                    })
                    .unwrap_or_default();
                return (blocks, range.end);
            }
            _ => {}
        }
    }
    (Vec::new(), source.len())
}

/// The blocks and directives of `events`, or `None` at a metadata block,
/// which isn't front matter after the very start.
fn read<'a>(
    source: &str,
    options: Options<'_>,
    events: impl Iterator<Item = (Event<'a>, Range<usize>)>,
) -> Option<Scanned> {
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
    for (event, range) in events {
        if matches!(event, Event::Start(Tag::MetadataBlock(_))) {
            return None;
        }
        s.event(event, range.start, range.end);
    }
    s.close();
    Some(Scanned {
        blocks: s.blocks,
        directives: s.directives,
    })
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
            Tag::HtmlBlock
            | Tag::Paragraph
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
            Tag::HtmlBlock => true,
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
        let src = "---\nlayout: utilize\n---\n\nProse.\n\n```\nutilize\n```\n\n    indented code\n\n<div>\nutilize\n</div>\n";
        assert_eq!(prose(src), ["Prose."]);
    }

    #[test]
    fn front_matter_is_read() {
        let src = "---\ntitle: A very unique title\nlayout: utilize\n---\n\n# One\n\nText.\n";
        let blocks = scan(src, Options::default()).blocks;
        assert_eq!(
            levels(&blocks),
            [
                ("A very unique title", Some(0)),
                ("One", Some(1)),
                ("Text.", None)
            ]
        );
        let b = &blocks[0];
        let i = b.text.find("very unique").unwrap();
        assert_eq!(&src[b.source_range(src, i..i + 11)], "very unique");

        let src = "+++\r\ndescription = \"In order to start.\"\r\n+++\r\n\r\nText.\r\n";
        let blocks = scan(src, Options::default()).blocks;
        assert_eq!(texts(&blocks), ["In order to start.", "Text."]);
        assert_eq!(blocks[0].kind, Scope::Paragraph);
        let i = blocks[0].text.find("start").unwrap();
        assert_eq!(&src[blocks[0].source_range(src, i..i + 5)], "start");
    }

    #[test]
    fn front_matter_is_only_at_the_start() {
        // A `---` after the start is a thematic break, and a line under
        // the next `---` is a heading, however a metadata block would end.
        assert_eq!(prose("Text.\n\n---\nNext.\n\n---\n"), ["Text.", "Next."]);
        assert_eq!(
            prose("---\n\nText.\n\n---\nNext.\n\n...\n"),
            ["Text.", "Next.", "..."]
        );
        assert_eq!(prose("\n---\ntitle: x\n---\n"), ["title: x"]);
        assert_eq!(
            prose("+++\n\nText.\n\n+++\nNext.\n+++\n"),
            ["+++", "Text.", "+++ Next. +++"]
        );
    }

    #[test]
    fn the_body_after_front_matter_is_read_again_in_place() {
        let src = "---\ntitle: A title\n---\n\n---\nNext.\n\n<!-- proserunner-disable -->\n\n---\n";
        let scanned = scan(src, Options::default());
        assert_eq!(texts(&scanned.blocks), ["A title", "Next."]);
        let next = &scanned.blocks[1];
        assert_eq!(&src[next.source_range(src, 0..5)], "Next.");
        let action = &scanned.directives[0].action;
        assert_eq!(&src[action.at..action.at + 19], "proserunner-disable");
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
