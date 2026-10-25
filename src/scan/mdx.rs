//! MDX through `markdown` (markdown-rs), which reads it as `@mdx-js/mdx`
//! does: CommonMark with GFM tables, footnotes, strikethrough and task
//! lists, and front matter, plus ESM, JSX and expressions. Prose is what
//! it is in Markdown, and the content of JSX elements is prose too, unless
//! the element is HTML that holds none, such as `<code>` or `<script>`,
//! and so are the strings `front_matter` reads. ESM (`import`, `export`)
//! and expressions (`{…}`) aren't prose; an expression in a sentence is a
//! placeholder. Inline control directives go in comment expressions:
//! `{/* proserunner-disable */}`.
//!
//! A file that isn't valid MDX is read as Markdown.

use markdown::mdast::Node;
use markdown::message::Message;
use markdown::{Constructs, MdxSignal, ParseOptions, to_mdast};

use super::control::{self, Directive};
use super::decoded::{self, Escapes};
use super::front_matter::{self, Syntax};
use super::view::ViewBuilder;
use super::{Block, Options, Scanned, Scope, html, markdown as commonmark};
use crate::contain;

pub(super) fn scan(source: &str, options: Options<'_>) -> Scanned {
    // markdown-rs panics on some input, such as `<A>x⏎---⏎`.
    let Some(Ok(root)) = contain::catch(|| parse(source)) else {
        return commonmark::scan(source, options);
    };
    let mut s = Scanner {
        source,
        options,
        blocks: Vec::new(),
        directives: Vec::new(),
    };
    s.children(&root, Scope::Paragraph);
    Scanned {
        blocks: s.blocks,
        directives: s.directives,
    }
}

fn parse(source: &str) -> Result<Node, Message> {
    let options = ParseOptions {
        constructs: Constructs {
            frontmatter: true,
            gfm_footnote_definition: true,
            gfm_label_start_footnote: true,
            gfm_strikethrough: true,
            gfm_table: true,
            gfm_task_list_item: true,
            ..Constructs::mdx()
        },
        // Without a parser for ESM, `import` and `export` lines would be
        // paragraphs.
        mdx_esm_parse: Some(Box::new(esm)),
        ..ParseOptions::default()
    };
    to_mdast(source, &options)
}

/// Whether `code`, ESM read so far, is whole. ESM is never prose, so it
/// isn't parsed: as markdown-rs does for an expression when it has no
/// parser for it, ESM ends where its brackets balance.
fn esm(code: &str) -> MdxSignal {
    let depth = code.bytes().fold(0_i64, |depth, b| match b {
        b'{' | b'(' | b'[' => depth + 1,
        b'}' | b')' | b']' => depth - 1,
        _ => depth,
    });
    if depth > 0 {
        MdxSignal::Eof(
            "Unexpected end of file in ESM".into(),
            Box::new("proserunner".into()),
            Box::new("esm".into()),
        )
    } else {
        MdxSignal::Ok
    }
}

struct Scanner<'src> {
    source: &'src str,
    options: Options<'src>,
    blocks: Vec<Block>,
    directives: Vec<Directive>,
}

impl Scanner<'_> {
    fn children(&mut self, node: &Node, scope: Scope) {
        for child in node.children().into_iter().flatten() {
            self.block(child, scope);
        }
    }

    fn block(&mut self, node: &Node, scope: Scope) {
        match node {
            Node::Paragraph(_) => self.prose(node, scope, None),
            Node::Heading(h) => self.prose(node, Scope::Heading, Some(h.depth)),
            Node::List(_) | Node::ListItem(_) => self.children(node, Scope::List),
            Node::Blockquote(_) => self.children(node, Scope::Blockquote),
            Node::TableCell(_) => self.prose(node, Scope::Table, None),
            Node::Code(code) if self.options.code_blocks => {
                let mut view = ViewBuilder::new(Scope::Code, None);
                if let Some((start, end)) = range(node) {
                    match self.fenced(start, end, &code.value) {
                        Some(at) => view.push_source(&code.value, at),
                        None => view.push_rewritten(&code.value, start..end),
                    }
                }
                self.blocks.extend(view.finish());
            }
            Node::Yaml(yaml) => self.front_matter(node, &yaml.value, Syntax::Yaml),
            Node::Toml(toml) => self.front_matter(node, &toml.value, Syntax::Toml),
            Node::MdxJsxFlowElement(element) => match element.name.as_deref() {
                Some(name) if html::is_opaque(name) => {}
                _ => self.children(node, scope),
            },
            Node::MdxFlowExpression(_) => self.directive(node),
            Node::Root(_) | Node::Table(_) | Node::TableRow(_) | Node::FootnoteDefinition(_) => {
                self.children(node, scope)
            }
            // ESM, code, math, definitions and breaks.
            _ => {}
        }
    }

    /// Front matter whose text is `value`.
    fn front_matter(&mut self, node: &Node, value: &str, syntax: Syntax) {
        let Some(at) = range(node).and_then(|(start, end)| self.fenced(start, end, value)) else {
            return;
        };
        let blocks = front_matter::blocks(syntax, value, at, self.options.front_matter);
        self.blocks.extend(blocks);
    }

    /// Where `value`, the text of the fenced node at `start..end`, starts
    /// in the source: after the opening fence's line, so a fence's own
    /// characters are never taken for it. MDX has no indented code.
    fn fenced(&self, start: usize, end: usize, value: &str) -> Option<usize> {
        let raw = self.source.get(start..end)?;
        let open = raw.find('\n')? + 1;
        raw[open..].find(value).map(|k| start + open + k)
    }

    /// A block of prose: `node`'s inline content.
    fn prose(&mut self, node: &Node, kind: Scope, level: Option<u8>) {
        let mut view = ViewBuilder::new(kind, level);
        self.inlines(&mut view, node);
        self.blocks.extend(view.finish());
    }

    fn inlines(&mut self, view: &mut ViewBuilder, node: &Node) {
        for child in node.children().into_iter().flatten() {
            let Some((start, end)) = range(child) else {
                continue;
            };
            match child {
                Node::Text(text) => self.text(view, &text.value, start, end),
                Node::Emphasis(_)
                | Node::Strong(_)
                | Node::Delete(_)
                | Node::Link(_)
                | Node::LinkReference(_) => self.inlines(view, child),
                Node::Break(_) => view.space(start),
                Node::MdxJsxTextElement(element) => match element.name.as_deref() {
                    Some(name) if html::is_opaque(name) => view.placeholder(start),
                    _ => self.inlines(view, child),
                },
                Node::MdxTextExpression(_) => {
                    self.directive(child);
                    view.placeholder(start);
                }
                // Code, math, images, footnote references.
                _ => view.placeholder(start),
            }
        }
    }

    /// `value`, text that markdown-rs read from source `start..end`, with
    /// escapes and references decoded and indentation left out.
    fn text(&self, view: &mut ViewBuilder, value: &str, start: usize, end: usize) {
        match self.source.get(start..end) {
            Some(raw) => decoded::push(view, value, raw, start, Escapes::Markdown),
            None => view.push_rewritten(value, start..end),
        }
    }

    /// A directive in a comment expression: `{/* proserunner-… */}`.
    fn directive(&mut self, node: &Node) {
        let Some((start, end)) = range(node) else {
            return;
        };
        let Some(raw) = self.source.get(start..end) else {
            return;
        };
        let inner = raw.trim_start_matches('{').trim_end_matches('}').trim();
        let Some(body) = inner.strip_prefix("/*").and_then(|b| b.strip_suffix("*/")) else {
            return;
        };
        let at = start + raw.find("/*").map_or(0, |k| k + 2);
        if let Some(d) = control::parse(body, end, |k| at + k) {
            self.directives.push(d);
        }
    }
}

/// `node`'s source range.
fn range(node: &Node) -> Option<(usize, usize)> {
    node.position().map(|p| (p.start.offset, p.end.offset))
}

#[cfg(test)]
mod tests {
    use super::super::tests::{levels, texts};
    use super::*;

    fn prose(src: &str) -> Vec<String> {
        texts(&scan(src, Options::default()).blocks)
            .into_iter()
            .map(|t| t.replace('\u{1A}', "#"))
            .collect()
    }

    #[test]
    fn esm_jsx_and_expressions() {
        let src = "import {Chart} from './chart.js'\nexport const meta = {\n  title: 'x',\n  tags: [\n    {name: 'utilize'}\n  ]\n}\n\n# A *very* title\n\nText with {props.year} and <Kbd>Ctrl</Kbd>, <code>utilize()</code>.\n\n<Note type=\"info\">\n  Inside a **note**.\n</Note>\n\n<Chart year={2024} />\n\n<script>{`utilize`}</script>\n";
        let blocks = scan(src, Options::default()).blocks;
        assert_eq!(
            levels(&blocks),
            [
                ("A very title", Some(1)),
                ("Text with \u{1A} and Ctrl, \u{1A}.", None),
                ("Inside a note.", None),
            ]
        );
    }

    #[test]
    fn markdown_constructs() {
        let src = "---\nlayout: utilize\n---\n\n- [ ] item\n\n> quoted\n\n| a | b |\n| - | - |\n| c | d |\n\nText.[^1] ~~gone~~\n\n[^1]: A note.\n\n```js\nutilize()\n```\n";
        assert_eq!(
            prose(src),
            [
                "item",
                "quoted",
                "a",
                "b",
                "c",
                "d",
                "Text.# gone",
                "A note."
            ]
        );
        let options = Options {
            code_blocks: true,
            ..Options::default()
        };
        let blocks = scan(src, options).blocks;
        let code = blocks.iter().find(|b| b.kind == Scope::Code).unwrap();
        assert_eq!(
            &src[code.source_range(src, 0..code.text.len())],
            "utilize()"
        );
    }

    #[test]
    fn front_matter_is_read() {
        let src = "---\r\ntitle: A very unique title\r\nlayout: utilize\r\n---\r\n\nText.\n";
        let blocks = scan(src, Options::default()).blocks;
        assert_eq!(
            levels(&blocks),
            [("A very unique title", Some(0)), ("Text.", None)]
        );
        let b = &blocks[0];
        let i = b.text.find("very unique").unwrap();
        assert_eq!(&src[b.source_range(src, i..i + 11)], "very unique");

        let src = "+++\ndescription = 'In order to start.'\n+++\n\nText.\n";
        assert_eq!(prose(src), ["In order to start.", "Text."]);
    }

    #[test]
    fn front_matter_in_invalid_mdx_is_read_as_markdown() {
        let src = "---\ntitle: A title\n---\n\nText with <Unclosed.\n";
        assert_eq!(prose(src), ["A title", "Text with <Unclosed."]);
    }

    #[test]
    fn code_maps_past_its_fence() {
        let options = Options {
            code_blocks: true,
            ..Options::default()
        };
        let src = "```js\njs\n```\n";
        let b = &scan(src, options).blocks[0];
        assert_eq!(b.source_range(src, 0..2), 6..8);
    }

    #[test]
    fn decoded_text_maps_to_source() {
        let src = "- caf&eacute; \\*not\n  very unique.\n";
        let b = &scan(src, Options::default()).blocks[0];
        assert_eq!(b.text, "café *not very unique.");
        let i = b.text.find("very unique").unwrap();
        assert_eq!(&src[b.source_range(src, i..i + 11)], "very unique");
        let i = b.text.find('é').unwrap();
        assert_eq!(&src[b.source_range(src, i..i + 2)], "&eacute;");
    }

    #[test]
    fn invalid_mdx_is_read_as_markdown() {
        let src = "Text with <Unclosed and utilize.\n";
        assert_eq!(prose(src), ["Text with <Unclosed and utilize."]);
    }

    #[test]
    fn mdx_markdown_rs_panics_on_is_read_as_markdown() {
        let src = "<Note>A title\n---\n";
        assert_eq!(
            levels(&scan(src, Options::default()).blocks),
            [("\u{1A}A title", Some(2))]
        );
    }

    #[test]
    fn directives_in_comment_expressions() {
        let src =
            "{/* proserunner-disable a/b -- why */}\n\nText {/* proserunner-enable */} here.\n";
        let scanned = scan(src, Options::default());
        let found: Vec<_> = scanned
            .directives
            .iter()
            .map(|d| d.action.text.as_str())
            .collect();
        assert_eq!(found, ["disable", "enable"]);
        assert_eq!(&src[scanned.directives[0].ids[0].at..][..3], "a/b");
    }
}
