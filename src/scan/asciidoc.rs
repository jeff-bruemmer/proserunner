//! AsciiDoc through `asciidoc-parser`, which reads it as Asciidoctor does.
//! Prose: the document title and section titles, paragraphs, block titles
//! (`.Title`), list items (without their checkboxes), description list
//! terms and definitions, a block per table cell, admonitions, quotes and
//! verses, sidebars, example and open blocks, and footnote text, as a
//! paragraph of its own. Listing, literal and source blocks, and indented
//! literal paragraphs, are code. Comments, the header's author and
//! revision lines, attribute entries, passthrough and stem blocks, and
//! block macros are skipped. Inline control directives go in `//` comment
//! lines.
//!
//! Inline markup goes the way Asciidoctor renders it: bold, italic,
//! highlight, superscript and subscript marks drop out, links and cross
//! references show their text, and `` "`curved`" `` quotes are quotes.
//! Monospace, passthroughs, attribute references, footnotes, and macros
//! such as `kbd:[…]` and `image:…[]` become placeholders. Typographic
//! replacements keep what the author typed: `it's` stays `it's`.
//!
//! Preprocessor directives (`ifdef::` … `endif::`, `include::`) are read as
//! comment lines, so every branch of a conditional is checked and nothing
//! is included.

use std::borrow::Cow;
use std::iter;
use std::ops::Range;

use asciidoc_parser::blocks::{
    self as adoc, ColumnStyle, ContentModel, FindBlocks, IsBlock, ListItemMarker, SimpleBlockStyle,
    TableCellContent,
};
use asciidoc_parser::inlines::{CharRef, InlineNode, RefVariant, StyleVariant};
use asciidoc_parser::parser::{Fidelity, ModificationContext, SourceMap};
use asciidoc_parser::{Document, HasSpan, Parser, Span};

use super::control::{self, Directive};
use super::html;
use super::view::ViewBuilder;
use super::{Block, Options, Scanned, Scope};

/// Read before a section title, which is parsed again on its own to get
/// its inline markup: `{empty}` stands for nothing, and keeps the title
/// from reading as a list item or other block.
const TITLE_PREFIX: &str = "{empty}";

/// What each typographic replacement is typed as.
const REPLACEMENTS: [(char, &str); 10] = [
    ('©', "(C)"),
    ('®', "(R)"),
    ('™', "(TM)"),
    ('—', "--"),
    ('…', "..."),
    ('→', "->"),
    ('⇒', "=>"),
    ('←', "<-"),
    ('⇐', "<="),
    ('’', "'"),
];

/// The lines that start preprocessor directives.
const DIRECTIVES: [&str; 5] = ["ifdef::", "ifndef::", "ifeval::", "endif::", "include::"];

pub(super) fn scan(source: &str, options: Options<'_>) -> Scanned {
    let read = without_directives(source);
    let doc = parse(&read);
    let s = Scanner {
        source,
        options,
        offsets: Offsets::new(source, doc.span().data(), doc.source_map()),
    };
    let mut out = Out::default();
    if let Some(title) = doc.header().title_source() {
        s.title(&mut out, title, 0);
    }
    for block in doc.child_blocks() {
        s.block(&mut out, block, Scope::Paragraph);
    }
    Scanned {
        directives: directives(source, &out.opaque),
        blocks: out.blocks,
    }
}

fn parse(text: &str) -> Document<'static> {
    // An undefined attribute's reference leaves a gap between the nodes
    // around it, which reads as a placeholder. UI macros such as `kbd:[…]`
    // are on unless the document turns them off.
    Parser::default()
        .with_intrinsic_attribute("attribute-missing", "drop", ModificationContext::Anywhere)
        .with_intrinsic_attribute_bool("experimental", true, ModificationContext::Anywhere)
        .parse(text)
}

/// `source` with every preprocessor directive turned into a comment line of
/// the same length, so the parser reads every line, and at its offset.
fn without_directives(source: &str) -> Cow<'_, str> {
    let is_directive = |line: &str| {
        DIRECTIVES.iter().any(|d| line.starts_with(d)) && line.trim_end().ends_with(']')
    };
    if !source.split('\n').any(is_directive) {
        return Cow::Borrowed(source);
    }
    let mut out = String::with_capacity(source.len());
    for line in source.split_inclusive('\n') {
        match line.get(2..) {
            Some(rest) if is_directive(line) => {
                out.push_str("//");
                out.push_str(rest);
            }
            _ => out.push_str(line),
        }
    }
    Cow::Owned(out)
}

/// Comment lines that hold directives: `//` at the start of a line, but
/// not in `opaque` source ranges, and not `///`, which delimits a comment
/// block.
fn directives(source: &str, opaque: &[Range<usize>]) -> Vec<Directive> {
    let mut out = Vec::new();
    let mut at = 0;
    for raw in source.split_inclusive('\n') {
        let line = raw.trim_end();
        if let Some(body) = line.strip_prefix("//")
            && !body.starts_with('/')
            && !opaque.iter().any(|r| r.contains(&at))
            && let Some(d) = control::parse(body, at + line.len(), |k| at + 2 + k)
        {
            out.push(d);
        }
        at += raw.len();
    }
    out
}

/// Maps offsets in the text the parser read to offsets in the source.
struct Offsets {
    /// Where each line the parser read starts, and where it starts in the
    /// source if it's there unchanged. Empty when the parser read the
    /// source as it is.
    lines: Vec<(usize, Option<usize>)>,
}

impl Offsets {
    fn new(source: &str, read: &str, map: &SourceMap) -> Offsets {
        if source.starts_with(read) {
            return Offsets { lines: Vec::new() };
        }
        let starts = line_starts(source);
        let lines = line_starts(read)
            .into_iter()
            .enumerate()
            .map(|(i, at)| {
                let origin = map.origin_at(i + 1, 1);
                let unchanged = origin.file.is_none() && origin.fidelity == Fidelity::Verbatim;
                let to = origin.line.checked_sub(1).and_then(|l| starts.get(l));
                (at, to.copied().filter(|_| unchanged))
            })
            .collect();
        Offsets { lines }
    }

    /// Offsets in a section title parsed on its own, after
    /// `TITLE_PREFIX`, which starts at source offset `at`.
    fn title(at: usize) -> Offsets {
        Offsets {
            lines: vec![(TITLE_PREFIX.len(), Some(at))],
        }
    }

    /// The source offset of offset `at` in what the parser read.
    fn get(&self, at: usize) -> Option<usize> {
        if self.lines.is_empty() {
            return Some(at);
        }
        let i = self.lines.partition_point(|l| l.0 <= at).checked_sub(1)?;
        let (start, to) = self.lines.get(i)?;
        Some((*to)? + (at - start))
    }

    /// The source range of `span`.
    fn range(&self, span: &Span<'_>) -> Option<Range<usize>> {
        let start = self.get(span.byte_offset())?;
        let end = match span.data().len() {
            0 => start,
            len => self.get(span.byte_offset() + len - 1)? + 1,
        };
        (start <= end).then_some(start..end)
    }
}

fn line_starts(text: &str) -> Vec<usize> {
    let ends = text.match_indices('\n').map(|(i, _)| i + 1);
    iter::once(0).chain(ends).collect()
}

struct Scanner<'src> {
    source: &'src str,
    options: Options<'src>,
    offsets: Offsets,
}

/// What a scan finds.
#[derive(Default)]
struct Out {
    blocks: Vec<Block>,
    /// Source ranges of code and raw blocks, where `//` starts no comment.
    opaque: Vec<Range<usize>>,
}

/// A block of prose being read from inline nodes.
struct Prose<'n, 'src> {
    view: ViewBuilder,
    /// Every text node's span is its text. Otherwise, as when the parser
    /// reindented a paragraph, text is found by looking ahead in the source.
    exact: bool,
    /// Where in the source reading has got to.
    cursor: usize,
    end: usize,
    /// The text of footnotes, each a paragraph of its own after this block.
    footnotes: Vec<&'n [InlineNode<'src>]>,
}

impl Scanner<'_> {
    fn block(&self, out: &mut Out, block: &adoc::Block<'_>, scope: Scope) {
        if block.declared_style() == Some("comment") {
            out.opaque.extend(self.offsets.range(&block.span()));
            return;
        }
        if let Some(title) = block.title_content() {
            self.prose(out, &self.offsets, title.inlines(), Scope::Paragraph, None);
        }
        match block {
            adoc::Block::Section(section) => {
                let level = u8::try_from(section.level()).unwrap_or(u8::MAX);
                self.title(out, section.section_title_source(), level);
                self.children(out, block, Scope::Paragraph);
            }
            adoc::Block::Simple(simple) => match simple.style() {
                SimpleBlockStyle::Paragraph => {
                    self.prose(out, &self.offsets, simple.content().inlines(), scope, None);
                }
                _ => self.code(out, block, &simple.content().original()),
            },
            adoc::Block::List(_) => self.children(out, block, Scope::List),
            adoc::Block::ListItem(item) => {
                if let ListItemMarker::DefinedTerm { term, .. } = item.list_item_marker() {
                    self.prose(out, &self.offsets, term.inlines(), Scope::List, None);
                }
                self.children(out, block, Scope::List);
            }
            adoc::Block::RawDelimited(raw) => match block.content_model() {
                ContentModel::Verbatim => self.code(out, block, &raw.content().original()),
                _ => out.opaque.extend(self.offsets.range(&block.span())),
            },
            adoc::Block::Admonition(admonition) => {
                if let Some(content) = admonition.content() {
                    self.prose(
                        out,
                        &self.offsets,
                        content.inlines(),
                        Scope::Paragraph,
                        None,
                    );
                }
                self.children(out, block, Scope::Paragraph);
            }
            adoc::Block::Quote(quote) => {
                if let Some(content) = quote.content() {
                    self.prose(
                        out,
                        &self.offsets,
                        content.inlines(),
                        Scope::Blockquote,
                        None,
                    );
                }
                self.children(out, block, Scope::Blockquote);
            }
            adoc::Block::Table(table) => {
                let rows = table.header_row().into_iter().chain(table.body_rows());
                for cell in rows.chain(table.footer_row()).flat_map(|r| r.cells()) {
                    match (cell.style(), cell.content()) {
                        (ColumnStyle::Literal | ColumnStyle::Monospace, _) => {}
                        (_, TableCellContent::Simple(content)) => {
                            self.prose(out, &self.offsets, content.inlines(), Scope::Table, None);
                        }
                        (_, TableCellContent::AsciiDoc(cell)) => {
                            for inner in cell.blocks() {
                                self.block(out, inner, Scope::Table);
                            }
                        }
                    }
                }
            }
            adoc::Block::Media(_)
            | adoc::Block::Break(_)
            | adoc::Block::Toc(_)
            | adoc::Block::DocumentAttribute(_) => {}
            // Example, sidebar and open blocks, the preamble, and blocks a
            // later version adds.
            _ => self.children(out, block, scope),
        }
    }

    fn children(&self, out: &mut Out, block: &adoc::Block<'_>, scope: Scope) {
        for child in block.child_blocks() {
            self.block(out, child, scope);
        }
    }

    /// A section or document title, parsed on its own: the parser keeps a
    /// title's inline markup to itself.
    fn title(&self, out: &mut Out, span: Span<'_>, level: u8) {
        let Some(range) = self.offsets.range(&span) else {
            return;
        };
        let Some(text) = self.source.get(range.clone()) else {
            return;
        };
        // Words and spaces hold no markup, and most titles are just that.
        if text.chars().all(|c| c.is_alphanumeric() || c == ' ') {
            let mut view = ViewBuilder::new(Scope::Heading, Some(level));
            view.push_source(text, range.start);
            out.blocks.extend(view.finish());
            return;
        }
        let doc = parse(&format!("{TITLE_PREFIX}{text}"));
        let Some(adoc::Block::Simple(title)) = doc.child_blocks().next() else {
            return;
        };
        let nodes = title.content().inlines();
        let nodes = match nodes.first() {
            Some(n) if n.span().byte_offset() < TITLE_PREFIX.len() => &nodes[1..],
            _ => nodes,
        };
        self.prose(
            out,
            &Offsets::title(range.start),
            nodes,
            Scope::Heading,
            Some(level),
        );
    }

    /// Code: checked only on request.
    fn code(&self, out: &mut Out, block: &adoc::Block<'_>, content: &Span<'_>) {
        out.opaque.extend(self.offsets.range(&block.span()));
        if !self.options.code_blocks {
            return;
        }
        let Some(range) = self.offsets.range(content) else {
            return;
        };
        let mut view = ViewBuilder::new(Scope::Code, None);
        if let Some(text) = self.source.get(range.clone()) {
            view.push_source(text, range.start);
        }
        out.blocks.extend(view.finish());
    }

    /// A block of prose from `nodes`, then a paragraph per footnote in it.
    fn prose(
        &self,
        out: &mut Out,
        offsets: &Offsets,
        nodes: &[InlineNode<'_>],
        kind: Scope,
        level: Option<u8>,
    ) {
        let (Some(first), Some(last)) = (nodes.first(), nodes.last()) else {
            return;
        };
        let start = offsets.range(&first.span()).map(|r| r.start);
        let end = offsets.range(&last.span()).map(|r| r.end);
        let (Some(start), Some(end)) = (start, end) else {
            return;
        };
        let mut p = Prose {
            view: ViewBuilder::new(kind, level),
            exact: nodes.iter().all(exact),
            cursor: start,
            end: end.max(start),
            footnotes: Vec::new(),
        };
        self.inlines(offsets, &mut p, nodes);
        out.blocks.extend(p.view.finish());
        for note in p.footnotes {
            self.prose(out, offsets, note, Scope::Paragraph, None);
        }
    }

    fn inlines<'n, 'src>(
        &self,
        offsets: &Offsets,
        p: &mut Prose<'n, 'src>,
        nodes: &'n [InlineNode<'src>],
    ) {
        for node in nodes {
            let range = offsets.range(&node.span()).filter(|_| p.exact);
            if let Some(r) = &range {
                self.gap(p, r.start);
            }
            let at = range.as_ref().map_or(p.cursor, |r| r.start);
            match node {
                InlineNode::Text { value, location } => self.text(offsets, p, value, location),
                InlineNode::CharRef { value, location } => {
                    self.char_ref(offsets, p, value, location, range.clone());
                }
                InlineNode::Styled(styled) => match styled.variant {
                    StyleVariant::Code if p.exact => p.view.placeholder(at),
                    StyleVariant::Code => {
                        // Find the code, to read on after it.
                        let mut found = None;
                        for child in &styled.children {
                            if let InlineNode::Text { value, .. } = child {
                                found = found.or(self.find(p, value));
                            }
                        }
                        p.view.placeholder(found.unwrap_or(at));
                    }
                    StyleVariant::DoubleQuote | StyleVariant::SingleQuote => {
                        let double = styled.variant == StyleVariant::DoubleQuote;
                        let (open, close) = if double {
                            ("“", "”")
                        } else {
                            ("‘", "’")
                        };
                        let ends = range
                            .as_ref()
                            .map(|r| (r.start..r.start + 2, r.end - 2..r.end));
                        self.rewritten(p, open, ends.clone().map(|e| e.0));
                        self.inlines(offsets, p, &styled.children);
                        self.rewritten(p, close, ends.map(|e| e.1));
                    }
                    _ => self.inlines(offsets, p, &styled.children),
                },
                InlineNode::Ref(link) => {
                    // A bare URL's text is the URL.
                    let bare = matches!(
                        link.children.as_slice(),
                        [InlineNode::Text { value, .. }] if value.as_ref() == link.target.as_ref()
                    );
                    if link.children.is_empty() || (link.variant == RefVariant::Link && bare) {
                        p.view.placeholder(at);
                    } else {
                        self.inlines(offsets, p, &link.children);
                    }
                }
                InlineNode::Footnote(note) => {
                    p.view.placeholder(at);
                    if !note.children.is_empty() {
                        p.footnotes.push(&note.children);
                    }
                }
                InlineNode::IndexTerm(term) => {
                    if term.visible {
                        self.inlines(offsets, p, &term.children);
                    }
                }
                InlineNode::LineBreak { .. } => p.view.space(at),
                InlineNode::Anchor(_) => {}
                InlineNode::Raw { .. }
                | InlineNode::Image(_)
                | InlineNode::Ui(_)
                | InlineNode::Stem(_)
                | InlineNode::Callout(_) => p.view.placeholder(at),
            }
            if let Some(r) = range {
                p.cursor = p.cursor.max(r.end);
            }
        }
    }

    fn char_ref(
        &self,
        offsets: &Offsets,
        p: &mut Prose<'_, '_>,
        value: &CharRef<'_>,
        location: &Span<'_>,
        range: Option<Range<usize>>,
    ) {
        let typed = match (value, &range) {
            (_, Some(_)) => Cow::Borrowed(location.data()),
            (CharRef::Special(c), None) => Cow::Owned(c.to_string()),
            (CharRef::Entity(entity), None) => Cow::Borrowed(entity.as_ref()),
            (CharRef::Replacement(r), None) => {
                let shown = r.trim_matches(['\u{2009}', '\u{200B}']);
                let typed = REPLACEMENTS.iter().find(|(c, _)| shown.starts_with(*c));
                Cow::Borrowed(typed.map_or(*r, |(_, t)| *t))
            }
        };
        match value {
            // Typed for a curved apostrophe: `` `' ``.
            CharRef::Replacement(c) if typed.contains('`') => self.rewritten(p, c, range),
            CharRef::Entity(entity) => match (html::reference(entity), range) {
                (Some((_, decoded)), Some(r)) => self.rewritten(p, &decoded, Some(r)),
                (Some((_, decoded)), None) => {
                    let found = self.find(p, entity);
                    let at = found.unwrap_or(p.cursor);
                    p.view
                        .push_rewritten(&decoded, at..found.map_or(at, |k| k + entity.len()));
                }
                (None, _) => self.text(offsets, p, &typed, location),
            },
            _ => self.text(offsets, p, &typed, location),
        }
    }

    /// Source the parser left out between the cursor and `next`: a dropped
    /// attribute reference is a placeholder, and a line break a space.
    fn gap(&self, p: &mut Prose<'_, '_>, next: usize) {
        let Some(gap) = self.source.get(p.cursor..next) else {
            return;
        };
        if gap.contains('{') {
            p.view.placeholder(p.cursor);
        } else if gap.contains(char::is_whitespace) {
            p.view.space(p.cursor);
        }
        p.cursor = next;
    }

    fn text(&self, offsets: &Offsets, p: &mut Prose<'_, '_>, value: &str, span: &Span<'_>) {
        let at = offsets.get(span.byte_offset()).filter(|_| p.exact);
        if is_attribute(value, span.data()) {
            p.view.placeholder(at.unwrap_or(p.cursor));
            return;
        }
        if let Some(at) = at
            && self.source.get(at..at + value.len()) == Some(value)
        {
            p.view.push_source(value, at);
            p.cursor = at + value.len();
            return;
        }
        if let Some(at) = at
            && self.aligned(p, value, at, span.data())
        {
            return;
        }
        // Each line of the text, found in order in the source.
        for (i, line) in value.split('\n').enumerate() {
            if i > 0 || line.starts_with(char::is_whitespace) {
                p.view.space(p.cursor);
            }
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match self.find(p, line) {
                Some(k) => p.view.push_source(line, k),
                None => p.view.push_rewritten(line, p.cursor..p.cursor),
            }
        }
        if value.ends_with(char::is_whitespace) {
            p.view.space(p.cursor);
        }
    }

    /// Where `text` next is in the block's source, reading on after it.
    fn find(&self, p: &mut Prose<'_, '_>, text: &str) -> Option<usize> {
        let k = p.cursor + self.source.get(p.cursor..p.end)?.find(text)?;
        p.cursor = k + text.len();
        Some(k)
    }

    /// Reads `value`, the text of source `data` at `at`, line by line: each
    /// of its lines is the end of a line of `data`, in order, and the
    /// parser left out the other lines, such as comments. `false`, having
    /// read nothing, when that isn't so.
    fn aligned(&self, p: &mut Prose<'_, '_>, value: &str, at: usize, data: &str) -> bool {
        let mut lines = Vec::new();
        let mut rest = data.split_inclusive('\n').scan(at, |offset, line| {
            let start = *offset;
            *offset += line.len();
            Some((start, line.trim_end_matches(['\n', '\r'])))
        });
        for wanted in value.split('\n') {
            let Some((start, line)) = rest.find(|(_, line)| line.ends_with(wanted)) else {
                return false;
            };
            lines.push((start + line.len() - wanted.len(), wanted));
        }
        if lines
            .iter()
            .any(|&(k, w)| self.source.get(k..k + w.len()) != Some(w))
        {
            return false;
        }
        for (i, (k, wanted)) in lines.into_iter().enumerate() {
            if i > 0 {
                p.view.space(k);
            }
            p.view.push_source(wanted, k);
            p.cursor = k + wanted.len();
        }
        true
    }

    /// Text that stands for `source`, or for the cursor's place.
    fn rewritten(&self, p: &mut Prose<'_, '_>, text: &str, source: Option<Range<usize>>) {
        let source = source
            .filter(|r| self.source.get(r.clone()).is_some())
            .unwrap_or(p.cursor..p.cursor);
        p.view.push_rewritten(text, source);
    }
}

/// A defined attribute's value, in place of its reference.
fn is_attribute(value: &str, source: &str) -> bool {
    value != source && source.starts_with('{') && source.ends_with('}')
}

/// Every text node's span is its text, so spans place it exactly.
fn exact(node: &InlineNode<'_>) -> bool {
    match node {
        InlineNode::Text { value, location } => {
            value.as_ref() == location.data() || is_attribute(value, location.data())
        }
        InlineNode::Styled(s) => s.children.iter().all(exact),
        InlineNode::Ref(r) => r.children.iter().all(exact),
        InlineNode::Footnote(f) => f.children.iter().all(exact),
        InlineNode::IndexTerm(t) => t.children.iter().all(exact),
        _ => true,
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
    fn header_and_section_levels() {
        let src = "= The Title\nJane Doe <jane@example.com>\nv1.0, 2024-01-01\n:toc:\n\nPreamble.\n\n== One\n\n=== Two ===\n\n## Markdown style\n\n.A block title\nText.\n";
        let blocks = scan(src, Options::default()).blocks;
        assert_eq!(
            levels(&blocks),
            [
                ("The Title", Some(0)),
                ("Preamble.", None),
                ("One", Some(1)),
                ("Two", Some(2)),
                ("Markdown style", Some(1)),
                ("A block title", None),
                ("Text.", None),
            ]
        );
    }

    #[test]
    fn titles_are_not_blocks() {
        let src = "== 1. Introduction\n\n== NOTE: A *very* title\n";
        assert_eq!(prose(src), ["1. Introduction", "NOTE: A very title"]);
        let blocks = scan(src, Options::default()).blocks;
        let b = &blocks[1];
        let i = b.text.find("very").unwrap();
        assert_eq!(&src[b.source_range(src, i..i + 4)], "very");
    }

    #[test]
    fn lists() {
        let src = "* one\ncontinued\n** nested\n* [x] done\n. first\n2. second\n\nCPU:: The brain.\nRAM::\n  Memory.\n\n* item\n+\nAttached.\n";
        assert_eq!(
            kinds(src),
            [
                (Scope::List, "one continued".into()),
                (Scope::List, "nested".into()),
                (Scope::List, "done".into()),
                (Scope::List, "first".into()),
                (Scope::List, "second".into()),
                (Scope::List, "CPU".into()),
                (Scope::List, "The brain.".into()),
                (Scope::List, "RAM".into()),
                (Scope::List, "Memory.".into()),
                (Scope::List, "item".into()),
                (Scope::List, "Attached.".into()),
            ]
        );
    }

    #[test]
    fn delimited_blocks() {
        let src = "[source,ruby]\n----\nutilize\n----\n\n....\nutilize\n....\n\n////\nutilize\n////\n\n++++\n<p>utilize</p>\n++++\n\n____\nQuoted.\n____\n\n[NOTE]\n====\nNoted.\n====\n\n****\nAside.\n****\n\n[source]\n--\nutilize\n--\n\n--\nOpen.\n--\n\n NOTE: not an admonition, but literal\n\nWARNING: Careful.\n\n[quote, Someone]\nSaid this.\n\n[comment]\nutilize\n\n> Markdown quote.\n";
        assert_eq!(
            kinds(src),
            [
                (Scope::Blockquote, "Quoted.".into()),
                (Scope::Paragraph, "Noted.".into()),
                (Scope::Paragraph, "Aside.".into()),
                (Scope::Paragraph, "Open.".into()),
                (Scope::Paragraph, "Careful.".into()),
                (Scope::Blockquote, "Said this.".into()),
                (Scope::Blockquote, "Markdown quote.".into()),
            ]
        );
        let options = Options {
            code_blocks: true,
            ..Options::default()
        };
        let blocks = scan("----\nlet x = 1;\nx\n----\n", options).blocks;
        assert_eq!(texts(&blocks), ["let x = 1; x"]);
        assert_eq!(blocks[0].kind, Scope::Code);
    }

    #[test]
    fn tables_have_a_block_per_cell() {
        let src = "[cols=\"1,2\"]\n|===\n|Head |Other\n\n|a\n|b\nmore\n2+|spans\na|AsciiDoc cell\n|last\n|===\n\n,===\nx,\"y, z\"\n,===\n";
        assert_eq!(
            kinds(src),
            [
                (Scope::Table, "Head".into()),
                (Scope::Table, "Other".into()),
                (Scope::Table, "a".into()),
                (Scope::Table, "b more".into()),
                (Scope::Table, "spans".into()),
                (Scope::Table, "AsciiDoc cell".into()),
                (Scope::Table, "last".into()),
                (Scope::Table, "x".into()),
                (Scope::Table, "y, z".into()),
            ]
        );
    }

    #[test]
    fn inline_markup() {
        let src = "Some *very* _unique_ **bold**ly `code` text, a ``mono``, #mark# and [.role]#roled#, \
                   x^2^ H~2~O, {attr}, kbd:[Ctrl+C], https://example.com[the site], link:a.html[], \
                   <<id,Cross>>, <<id>>, [[anchor]]((indexed)), (((hidden))), +pass+, \\*not*, \
                   snake_case_name, 2*3*4.\n";
        assert_eq!(
            prose(src),
            [
                "Some very unique boldly # text, a #, mark and roled, x2 H2O, #, #, the site, #, \
              Cross, #, indexed, , #, *not*, snake_case_name, 2*3*4."
            ]
        );
    }

    #[test]
    fn replacements_keep_what_was_typed() {
        let src = "She said \"`obviously`\" it`'s fine -- it's (C) +\nhere...\n";
        let blocks = scan(src, Options::default()).blocks;
        assert_eq!(
            texts(&blocks),
            ["She said “obviously” it’s fine -- it's (C) here..."]
        );
    }

    #[test]
    fn defined_attributes_are_placeholders() {
        let src = ":product: Widget\n\nThe {product} and {missing} parts.\n";
        assert_eq!(prose(src), ["The # and # parts."]);
    }

    #[test]
    fn footnotes_are_paragraphs_of_their_own() {
        let src = "Main text.footnote:[The *very* unique note.] More.\n";
        assert_eq!(prose(src), ["Main text.# More.", "The very unique note."]);
        let scanned = scan(src, Options::default());
        let note = &scanned.blocks[1];
        let i = note.text.find("very unique").unwrap();
        assert_eq!(&src[note.source_range(src, i..i + 11)], "very* unique");
    }

    #[test]
    fn spans_map_to_source() {
        let src = "== A *very*\nunique title\n\nSome _very_\nunique text.\n";
        let blocks = scan(src, Options::default()).blocks;
        assert_eq!(
            texts(&blocks),
            ["A very", "unique title", "Some very unique text."]
        );
        let b = &blocks[2];
        let i = b.text.find("very unique").unwrap();
        assert_eq!(&src[b.source_range(src, i..i + 11)], "very_\nunique");
    }

    #[test]
    fn every_branch_of_a_conditional_is_read() {
        let src = "ifdef::env-github[]\nOn GitHub, we utilize.\nendif::[]\nifndef::env-github[]\nElsewhere, we utilize.\nendif::[]\n\ninclude::other.adoc[]\n\nLast.\n";
        let blocks = scan(src, Options::default()).blocks;
        // The lines between are comments, which don't end a paragraph.
        assert_eq!(
            texts(&blocks),
            ["On GitHub, we utilize. Elsewhere, we utilize.", "Last."]
        );
        let b = &blocks[0];
        let i = b.text.rfind("utilize").unwrap();
        assert_eq!(&src[b.source_range(src, i..i + 7)], "utilize");
    }

    #[test]
    fn reindented_text_is_found_in_the_source() {
        let src = "Term::\n  Some _very_\n  unique `code` text.\n";
        assert_eq!(prose(src), ["Term", "Some very unique # text."]);
        let blocks = scan(src, Options::default()).blocks;
        let b = &blocks[1];
        let i = b.text.find("unique").unwrap();
        assert_eq!(&src[b.source_range(src, i..i + 6)], "unique");
    }

    #[test]
    fn directives_in_comments() {
        let src = "// proserunner-disable a/b -- why\nText.\n// proserunner-enable\nmore.\n\n----\n// proserunner-disable\n----\n";
        let scanned = scan(src, Options::default());
        let found: Vec<_> = scanned
            .directives
            .iter()
            .map(|d| d.action.text.as_str())
            .collect();
        assert_eq!(found, ["disable", "enable"]);
        assert_eq!(&src[scanned.directives[0].ids[0].at..][..3], "a/b");
        assert_eq!(texts(&scanned.blocks), ["Text. more."]);
    }
}
