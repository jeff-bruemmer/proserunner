//! Typst through `typst-syntax`, the parser of the Typst compiler. Prose:
//! paragraphs, headings (`=` and `#heading`, and `#title` as the
//! document's title), list, enum and term items, table cells, figure
//! captions, quotes, and footnote text, as a paragraph of its own. Raw
//! blocks are code. Set, show and let rules, imports, and code are
//! skipped. Inline control directives go in `//` and `/* */` comments.
//!
//! Markup goes the way Typst renders it: `*strong*` and `_emphasis_` drop
//! their markers, and the content of a function call such as
//! `#emph[…]` or `#link("…")[…]` is prose. Inline raw text, math, links,
//! references, and calls with no content, such as `#image("…")`, become
//! placeholders. Shorthands keep what the author typed, except `~`, a
//! space.

use std::mem;

use typst_syntax::{LinkedNode, SyntaxKind as K, parse};

use super::control::{self, Directive};
use super::view::ViewBuilder;
use super::{Block, Options, Scanned, Scope};

pub(super) fn scan(source: &str, options: Options<'_>) -> Scanned {
    let root = parse(source);
    let root = LinkedNode::new(&root);
    let mut s = Scanner {
        source,
        options,
        current: None,
        footnotes: Vec::new(),
        blocks: Vec::new(),
        directives: Vec::new(),
    };
    s.flow(&root, Scope::Paragraph, None);
    s.comments(&root);
    Scanned {
        blocks: s.blocks,
        directives: s.directives,
    }
}

/// What a function call is to the prose around it.
#[derive(Clone, Copy)]
enum Call {
    /// Its content is inline prose: `#emph[…]`.
    Inline,
    /// A block of this scope, and level if it's a heading.
    Block(Scope, Option<u8>),
    /// Each content argument is a block of this scope: a table's cells.
    Cells(Scope),
    /// Space: `#h(1em)`, `#linebreak()`.
    Space,
    /// A placeholder, and a paragraph of its own for its content.
    Footnote,
    /// A quote, inline unless `block: true`.
    Quote,
    /// A figure: its body, then its caption as a paragraph.
    Figure,
}

fn call(name: &str) -> Call {
    match name {
        "heading" => Call::Block(Scope::Heading, Some(1)),
        "title" => Call::Block(Scope::Heading, Some(0)),
        "par" | "block" | "align" | "pad" | "columns" | "place" | "rect" | "square" | "circle"
        | "ellipse" | "stack" | "move" | "rotate" | "scale" => Call::Block(Scope::Paragraph, None),
        "list" | "enum" | "terms" => Call::Cells(Scope::List),
        "table" => Call::Cells(Scope::Table),
        "grid" => Call::Cells(Scope::Paragraph),
        "h" | "v" | "linebreak" | "parbreak" | "pagebreak" | "colbreak" => Call::Space,
        "footnote" => Call::Footnote,
        "quote" => Call::Quote,
        "figure" => Call::Figure,
        // Formatting, links, and functions a document defines.
        _ => Call::Inline,
    }
}

struct Scanner<'src, 'a> {
    source: &'src str,
    options: Options<'src>,
    /// The block being read.
    current: Option<ViewBuilder>,
    /// The content of footnotes in it, each a paragraph after it.
    footnotes: Vec<LinkedNode<'a>>,
    blocks: Vec<Block>,
    directives: Vec<Directive>,
}

impl<'a> Scanner<'_, 'a> {
    /// Markup that makes blocks of its own: each paragraph in it is a
    /// block of `kind`.
    fn flow(&mut self, markup: &LinkedNode<'a>, kind: Scope, level: Option<u8>) {
        self.close();
        self.current = Some(ViewBuilder::new(kind, level));
        self.items(markup, kind, level);
        self.close();
    }

    fn items(&mut self, node: &LinkedNode<'a>, kind: Scope, level: Option<u8>) {
        for child in node.children() {
            self.item(&child, kind, level);
        }
    }

    fn item(&mut self, node: &LinkedNode<'a>, kind: Scope, level: Option<u8>) {
        let at = node.offset();
        match node.kind() {
            K::Text | K::SmartQuote => self.push(node, kind, level),
            K::Space | K::Linebreak => self.space(at),
            K::Parbreak => {
                self.close();
                self.current = Some(ViewBuilder::new(kind, level));
            }
            K::Shorthand => match node.get().leaf_text().as_str() {
                "~" => self.space(at),
                // A soft hyphen joins the parts of a word.
                "-?" => {}
                _ => self.push(node, kind, level),
            },
            K::Escape => {
                let escaped = unescape(node.get().leaf_text());
                self.view(kind, level)
                    .push_rewritten(&escaped, node.range());
            }
            K::Strong | K::Emph | K::Markup | K::ContentBlock => self.items(node, kind, level),
            K::Raw if is_block_raw(node) => self.code(node),
            K::Raw | K::Link | K::Ref | K::Equation => self.view(kind, level).placeholder(at),
            K::Heading => {
                let marker = node.children().find(|c| c.kind() == K::HeadingMarker);
                let depth = marker.map_or(1, |m| m.range().len());
                if let Some(title) = child(node, K::Markup) {
                    self.flow(
                        &title,
                        Scope::Heading,
                        Some(u8::try_from(depth).unwrap_or(u8::MAX)),
                    );
                }
                self.resume(kind, level);
            }
            K::ListItem | K::EnumItem | K::TermItem => {
                for part in node.children().filter(|c| c.kind() == K::Markup) {
                    self.flow(&part, Scope::List, None);
                }
                self.resume(kind, level);
            }
            K::FuncCall => self.func_call(node, kind, level),
            // Code whose content shows: `#if x [shown] else [other]`.
            K::Conditional | K::ForLoop | K::WhileLoop | K::CodeBlock | K::Code => {
                self.items(node, kind, level);
            }
            // Comments, labels, set, show and let rules, imports, and code
            // with nothing to show.
            _ => {}
        }
    }

    fn func_call(&mut self, node: &LinkedNode<'a>, kind: Scope, level: Option<u8>) {
        let name = node
            .children()
            .find_map(|c| match c.kind() {
                K::Ident => Some(c.get().leaf_text().to_string()),
                // `table.cell`: the field.
                K::FieldAccess => c.children().last().map(|f| f.get().leaf_text().to_string()),
                _ => None,
            })
            .unwrap_or_default();
        let Some(args) = child(node, K::Args) else {
            return;
        };
        let content: Vec<LinkedNode<'a>> = args
            .children()
            .filter(|c| matches!(c.kind(), K::ContentBlock | K::FuncCall))
            .collect();
        let named = |key: &str| {
            args.children()
                .filter(|c| c.kind() == K::Named)
                .find_map(|n| {
                    let mut parts = n.children();
                    let ident = parts.next()?;
                    (ident.get().leaf_text() == key)
                        .then(|| parts.last())
                        .flatten()
                })
        };
        match call(&name) {
            Call::Inline if content.is_empty() => {
                self.view(kind, level).placeholder(node.offset());
            }
            Call::Inline => {
                for c in &content {
                    self.item(c, kind, level);
                }
            }
            Call::Block(block, heading) => {
                let heading = match named("level") {
                    Some(n) if heading.is_some() => n.get().leaf_text().parse().ok().or(heading),
                    _ => heading,
                };
                for c in &content {
                    self.flow(c, block, heading);
                }
                self.resume(kind, level);
            }
            Call::Cells(cells) => {
                for c in &content {
                    match c.kind() {
                        K::FuncCall => self.func_call(c, cells, None),
                        _ => self.flow(c, cells, None),
                    }
                }
                self.resume(kind, level);
            }
            Call::Space => self.space(node.offset()),
            Call::Footnote => {
                self.view(kind, level).placeholder(node.offset());
                self.footnotes.extend(content);
            }
            Call::Quote if named("block").is_some_and(|b| b.get().leaf_text() == "true") => {
                for c in &content {
                    self.flow(c, Scope::Blockquote, None);
                }
                self.resume(kind, level);
            }
            Call::Quote => {
                let range = node.range();
                self.view(kind, level)
                    .push_rewritten("\u{201C}", range.start..range.start + 1);
                for c in &content {
                    self.item(c, kind, level);
                }
                self.view(kind, level)
                    .push_rewritten("\u{201D}", range.end - 1..range.end);
            }
            Call::Figure => {
                for c in &content {
                    match c.kind() {
                        K::FuncCall => self.func_call(c, Scope::Paragraph, None),
                        _ => self.flow(c, Scope::Paragraph, None),
                    }
                }
                if let Some(caption) = named("caption").filter(|c| c.kind() == K::ContentBlock) {
                    self.flow(&caption, Scope::Paragraph, None);
                }
                self.resume(kind, level);
            }
        }
    }

    /// Raw text that's a block: code, checked only on request.
    fn code(&mut self, raw: &LinkedNode<'a>) {
        self.close();
        if self.options.code_blocks {
            let mut view = ViewBuilder::new(Scope::Code, None);
            for text in raw.children().filter(|c| c.kind() == K::Text) {
                if let Some(t) = self.source.get(text.range()) {
                    view.push_source(t, text.offset());
                }
            }
            self.blocks.extend(view.finish());
        }
    }

    /// Directives in comments, anywhere in the document.
    fn comments(&mut self, node: &LinkedNode<'a>) {
        for child in node.children() {
            let text = child.get().leaf_text();
            let body = match child.kind() {
                K::LineComment => text.strip_prefix("//"),
                K::BlockComment => text
                    .strip_prefix("/*")
                    .map(|t| t.strip_suffix("*/").unwrap_or(t)),
                _ => {
                    self.comments(&child);
                    continue;
                }
            };
            let start = child.offset() + 2;
            if let Some(d) = body.and_then(|b| control::parse(b, child.range().end, |k| start + k))
            {
                self.directives.push(d);
            }
        }
    }

    fn push(&mut self, node: &LinkedNode<'a>, kind: Scope, level: Option<u8>) {
        if let Some(text) = self.source.get(node.range()) {
            self.view(kind, level).push_source(text, node.offset());
        }
    }

    fn space(&mut self, at: usize) {
        if let Some(view) = self.current.as_mut() {
            view.space(at);
        }
    }

    /// The open block, opening one of `kind` if none is.
    fn view(&mut self, kind: Scope, level: Option<u8>) -> &mut ViewBuilder {
        self.current
            .get_or_insert_with(|| ViewBuilder::new(kind, level))
    }

    /// After a block inside markup, the markup reads on in a new block.
    fn resume(&mut self, kind: Scope, level: Option<u8>) {
        self.close();
        self.current = Some(ViewBuilder::new(kind, level));
    }

    /// Ends the open block, and reads the footnotes in it.
    fn close(&mut self) {
        if let Some(block) = self.current.take().and_then(ViewBuilder::finish) {
            self.blocks.push(block);
        }
        for note in mem::take(&mut self.footnotes) {
            self.flow(&note, Scope::Paragraph, None);
        }
    }
}

/// `node`'s first child of `kind`.
fn child<'a>(node: &LinkedNode<'a>, kind: K) -> Option<LinkedNode<'a>> {
    node.children().find(|c| c.kind() == kind)
}

/// Raw text in three or more backticks, over more than one line: a block.
fn is_block_raw(raw: &LinkedNode<'_>) -> bool {
    let fence = child(raw, K::RawDelim).is_some_and(|d| d.range().len() >= 3);
    fence && raw.get().full_text().contains('\n')
}

/// What an escape stands for: `\#` is `#`, `\u{1F600}` is 😀.
fn unescape(escape: &str) -> String {
    let rest = escape.strip_prefix('\\').unwrap_or(escape);
    let code = rest
        .strip_prefix("u{")
        .and_then(|h| h.strip_suffix('}'))
        .and_then(|h| u32::from_str_radix(h, 16).ok());
    match code {
        Some(c) => char::from_u32(c).map(String::from).unwrap_or_default(),
        None => rest.to_owned(),
    }
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

    fn kinds(src: &str) -> Vec<(Scope, String)> {
        scan(src, Options::default())
            .blocks
            .into_iter()
            .map(|b| (b.kind, b.text.replace('\u{1A}', "#")))
            .collect()
    }

    #[test]
    fn headings_and_paragraphs() {
        let src = "#set page(width: 10cm)\n#let x = 1\n#title[The title]\n= One <intro>\nSome text\nwraps.\n\nNext.\n== Two\n#heading(level: 3)[Three]\n";
        let blocks = scan(src, Options::default()).blocks;
        assert_eq!(
            levels(&blocks),
            [
                ("The title", Some(0)),
                ("One", Some(1)),
                ("Some text wraps.", None),
                ("Next.", None),
                ("Two", Some(2)),
                ("Three", Some(3)),
            ]
        );
    }

    #[test]
    fn inline_markup() {
        let src = "Some *very* _unique_ text with `code`, $x^2$, @intro, https://typst.app, \
                   #emph[emphasized], #link(\"https://x.org\")[the docs], #image(\"a.png\"), \
                   #lorem(3), and~nbsp, ex-?ample, it's -- \\#1.\n";
        assert_eq!(
            prose(src),
            [
                "Some very unique text with #, #, #, #, emphasized, the docs, #, #, \
                 and nbsp, example, it's -- #1."
            ]
        );
    }

    #[test]
    fn lists_tables_figures_and_quotes() {
        let src = "- item one\n  - nested\n+ enum two\n/ Term: description\n\n#table(columns: 2, [Cell one], table.cell[Cell two])\n#figure(image(\"a.png\"), caption: [A caption.])\n#quote(block: true)[Quoted text.]\nShe said #quote[obviously] no.\n";
        assert_eq!(
            kinds(src),
            [
                (Scope::List, "item one".into()),
                (Scope::List, "nested".into()),
                (Scope::List, "enum two".into()),
                (Scope::List, "Term".into()),
                (Scope::List, "description".into()),
                (Scope::Table, "Cell one".into()),
                (Scope::Table, "Cell two".into()),
                (Scope::Paragraph, "A caption.".into()),
                (Scope::Blockquote, "Quoted text.".into()),
                (Scope::Paragraph, "She said “obviously” no.".into()),
            ]
        );
    }

    #[test]
    fn footnotes_are_paragraphs_of_their_own() {
        let src = "Main text#footnote[The *very* unique note.] more.\n";
        assert_eq!(prose(src), ["Main text# more.", "The very unique note."]);
        let blocks = scan(src, Options::default()).blocks;
        let note = &blocks[1];
        let i = note.text.find("very unique").unwrap();
        assert_eq!(&src[note.source_range(src, i..i + 11)], "very* unique");
    }

    #[test]
    fn code_blocks_on_request() {
        let src = "Prose.\n\n```rust\nlet utilize = 1;\n```\n";
        assert_eq!(prose(src), ["Prose."]);
        let options = Options {
            code_blocks: true,
            ..Options::default()
        };
        let blocks = scan(src, options).blocks;
        assert_eq!(texts(&blocks), ["Prose.", "let utilize = 1;"]);
        assert_eq!(blocks[1].kind, Scope::Code);
    }

    #[test]
    fn spans_map_to_source() {
        let src = "= A *very*\nSome _very_\nunique text.\n";
        let blocks = scan(src, Options::default()).blocks;
        let b = &blocks[1];
        let i = b.text.find("very unique").unwrap();
        assert_eq!(&src[b.source_range(src, i..i + 11)], "very_\nunique");
    }

    #[test]
    fn directives_in_comments() {
        let src = "// proserunner-disable a/b -- why\nText. /* proserunner-enable */ more.\n#figure(\n  // proserunner-disable-next-line\n  [x])\n";
        let scanned = scan(src, Options::default());
        let found: Vec<_> = scanned
            .directives
            .iter()
            .map(|d| d.action.text.as_str())
            .collect();
        assert_eq!(found, ["disable", "enable", "disable-next-line"]);
        assert_eq!(&src[scanned.directives[0].ids[0].at..][..3], "a/b");
        assert_eq!(texts(&scanned.blocks)[0], "Text. more.");
    }
}
