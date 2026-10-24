//! Org mode through `orgize`. Headlines, paragraphs, list items and their
//! tags, table cells, footnote definitions and the contents of quote,
//! verse and other prose blocks are prose. Source and example blocks and
//! `:` lines are code. Comments, keywords (except `#+TITLE` and
//! `#+SUBTITLE`, which are headings), drawers, planning lines, comment and
//! export blocks, and `COMMENT` subtrees are skipped. Inline control
//! directives go in `#` comment lines.
//!
//! Inline markup goes the way Org renders it: emphasis markers drop out
//! and a link shows its description, while verbatim, code, bare links,
//! timestamps, footnote references, cookies, macros, targets, math,
//! entities and export snippets become placeholders. Subscripts and
//! superscripts keep their source, so `snake_case` stays one word.

use orgize::ast::Headline;
use orgize::rowan::ast::AstNode;
use orgize::{Org, ParseConfig, SyntaxElement, SyntaxKind as K, SyntaxNode, TextRange};

use super::control::{self, Directive};
use super::view::ViewBuilder;
use super::{Block, Options, Scanned, Scope};

/// Read before text that the parser leaves unparsed, such as a title, so
/// that parsing it on its own finds a paragraph and not a list item.
const PREFIX: &str = "x ";

pub(super) fn scan(source: &str, options: Options<'_>) -> Scanned {
    let org = parse(source);
    let mut s = Scanner {
        source,
        options,
        blocks: Vec::new(),
        directives: Vec::new(),
    };
    s.children(org.document().syntax(), Origin::SOURCE, Scope::Paragraph);
    Scanned {
        blocks: s.blocks,
        directives: s.directives,
    }
}

/// `source` parsed with the TODO keywords its `#+TODO:` lines set. Those
/// lines are parsed first on their own, so the document is parsed once.
fn parse(source: &str) -> Org {
    let settings: Vec<&str> = source
        .lines()
        .filter(|l| l.trim_start().starts_with("#+") && l.to_ascii_uppercase().contains("TODO"))
        .collect();
    let keywords = (!settings.is_empty())
        .then(|| todo_keywords(Org::parse(settings.join("\n")).document().syntax()))
        .flatten();
    match keywords {
        Some(todo_keywords) => ParseConfig {
            todo_keywords,
            ..ParseConfig::default()
        }
        .parse(source),
        None => Org::parse(source),
    }
}

/// The keywords `#+TODO:` lines set, as (to do, done), if there are any.
/// Without a `|`, the last keyword is the done one; `NEXT(n)` is `NEXT`.
fn todo_keywords(document: &SyntaxNode) -> Option<(Vec<String>, Vec<String>)> {
    let mut keywords: Option<(Vec<String>, Vec<String>)> = None;
    for node in document.descendants().filter(|n| n.kind() == K::KEYWORD) {
        let mut texts = node
            .children_with_tokens()
            .filter_map(SyntaxElement::into_token)
            .filter(|t| t.kind() == K::TEXT);
        let Some(key) = texts.next() else { continue };
        let key = key.text().to_ascii_uppercase();
        if !matches!(key.as_str(), "TODO" | "SEQ_TODO" | "TYP_TODO") {
            continue;
        }
        let value = texts.next();
        let words: Vec<&str> = value
            .as_ref()
            .map_or("", |v| v.text())
            .split_whitespace()
            .collect();
        let split = words
            .iter()
            .position(|w| *w == "|")
            .unwrap_or(words.len().saturating_sub(1));
        let name = |w: &&str| w.split('(').next().unwrap_or(w).to_owned();
        let (todo, done) = keywords.get_or_insert_default();
        todo.extend(words[..split].iter().map(name));
        done.extend(words[split..].iter().filter(|w| **w != "|").map(name));
    }
    keywords
}

/// Where a parsed text's offsets come from: offset `skip` of the text
/// parsed is offset `at` of the source.
#[derive(Clone, Copy)]
struct Origin {
    at: usize,
    skip: usize,
}

impl Origin {
    /// The source itself.
    const SOURCE: Origin = Origin { at: 0, skip: 0 };

    /// The source range of `range` of the text parsed, cut to what's in
    /// the source.
    fn range(self, range: TextRange) -> (usize, usize) {
        let start = usize::from(range.start()).max(self.skip);
        let end = usize::from(range.end()).max(start);
        (self.at + start - self.skip, self.at + end - self.skip)
    }
}

struct Scanner<'src> {
    source: &'src str,
    options: Options<'src>,
    blocks: Vec<Block>,
    directives: Vec<Directive>,
}

impl Scanner<'_> {
    fn children(&mut self, node: &SyntaxNode, origin: Origin, scope: Scope) {
        for child in node.children() {
            self.element(&child, origin, scope);
        }
    }

    fn element(&mut self, node: &SyntaxNode, origin: Origin, scope: Scope) {
        match node.kind() {
            K::HEADLINE => self.headline(node, origin),
            K::PARAGRAPH => self.paragraph(node, origin, scope),
            K::LIST => self.children(node, origin, Scope::List),
            K::LIST_ITEM_TAG => self.prose(node, origin, Scope::List, None),
            K::ORG_TABLE_CELL => self.prose(node, origin, Scope::Table, None),
            K::QUOTE_BLOCK => self.content(node, origin, Scope::Blockquote),
            K::VERSE_BLOCK => {
                if let Some(content) = child(node, K::BLOCK_CONTENT) {
                    self.fragment(origin.range(content.text_range()), scope, None);
                }
            }
            K::SPECIAL_BLOCK if is_export(node) => {}
            K::SOURCE_BLOCK | K::EXAMPLE_BLOCK => {
                if let Some(content) = child(node, K::BLOCK_CONTENT) {
                    self.code(origin, &[content.text_range()]);
                }
            }
            K::FIXED_WIDTH => {
                let lines: Vec<TextRange> = node
                    .children_with_tokens()
                    .filter_map(SyntaxElement::into_token)
                    .filter(|t| t.kind() == K::TEXT)
                    .map(|t| t.text_range())
                    .collect();
                self.code(origin, &lines);
            }
            K::KEYWORD => self.keyword(node, origin),
            K::FN_DEF => {
                // The definition's text starts after its label's `]`.
                let label = node
                    .children_with_tokens()
                    .find(|e| e.kind() == K::R_BRACKET);
                if let Some(label) = label {
                    let range = TextRange::new(label.text_range().end(), node.text_range().end());
                    self.fragment(origin.range(range), Scope::Paragraph, None);
                }
            }
            K::COMMENT => self.comment(node, origin),
            K::CENTER_BLOCK | K::SPECIAL_BLOCK => self.content(node, origin, scope),
            K::DOCUMENT
            | K::SECTION
            | K::LIST_ITEM
            | K::LIST_ITEM_CONTENT
            | K::ORG_TABLE
            | K::ORG_TABLE_STANDARD_ROW => self.children(node, origin, scope),
            // Drawers, planning, clocks, comment and export blocks,
            // dynamic blocks, LaTeX environments, rules.
            _ => {}
        }
    }

    /// A paragraph. One that starts with affiliated keywords, such as
    /// `#+NAME:`, is read again without them: the parser takes a table
    /// after them for a paragraph.
    fn paragraph(&mut self, node: &SyntaxNode, origin: Origin, scope: Scope) {
        let keywords = node
            .children()
            .take_while(|c| c.kind() == K::AFFILIATED_KEYWORD)
            .last();
        let Some(keywords) = keywords else {
            self.prose(node, origin, scope, None);
            return;
        };
        let rest = TextRange::new(keywords.text_range().end(), node.text_range().end());
        let (start, end) = origin.range(rest);
        let Some(text) = self.source.get(start..end) else {
            return;
        };
        let org = Org::parse(text);
        let at = Origin { at: start, skip: 0 };
        self.children(org.document().syntax(), at, scope);
    }

    fn headline(&mut self, node: &SyntaxNode, origin: Origin) {
        if Headline::cast(node.clone()).is_some_and(|h| h.is_commented()) {
            return;
        }
        let stars = node
            .children_with_tokens()
            .find(|e| e.kind() == K::HEADLINE_STARS)
            .map_or(1, |e| usize::from(e.text_range().len()));
        let level = u8::try_from(stars).unwrap_or(u8::MAX);
        for child in node.children() {
            match child.kind() {
                K::HEADLINE_TITLE => self.prose(&child, origin, Scope::Heading, Some(level)),
                K::SECTION | K::HEADLINE => self.element(&child, origin, Scope::Paragraph),
                _ => {}
            }
        }
    }

    /// A block's contents.
    fn content(&mut self, node: &SyntaxNode, origin: Origin, scope: Scope) {
        if let Some(content) = child(node, K::BLOCK_CONTENT) {
            self.children(&content, origin, scope);
        }
    }

    /// `#+TITLE` and `#+SUBTITLE`, as headings; other keywords are
    /// skipped.
    fn keyword(&mut self, node: &SyntaxNode, origin: Origin) {
        let mut texts = node
            .children_with_tokens()
            .filter_map(SyntaxElement::into_token)
            .filter(|t| t.kind() == K::TEXT);
        let (Some(key), Some(value)) = (texts.next(), texts.next()) else {
            return;
        };
        if key.text().eq_ignore_ascii_case("TITLE") || key.text().eq_ignore_ascii_case("SUBTITLE") {
            self.fragment(origin.range(value.text_range()), Scope::Heading, Some(0));
        }
    }

    /// Source text the parser leaves unparsed, parsed on its own: each of
    /// its paragraphs is a block.
    fn fragment(&mut self, (start, end): (usize, usize), kind: Scope, level: Option<u8>) {
        let Some(text) = self.source.get(start..end) else {
            return;
        };
        let org = Org::parse(format!("{PREFIX}{text}"));
        let origin = Origin {
            at: start,
            skip: PREFIX.len(),
        };
        let paragraphs = org
            .document()
            .syntax()
            .descendants()
            .filter(|n| n.kind() == K::PARAGRAPH);
        for paragraph in paragraphs {
            self.prose(&paragraph, origin, kind, level);
        }
    }

    /// A block of prose: the text and inline objects of `node`.
    fn prose(&mut self, node: &SyntaxNode, origin: Origin, kind: Scope, level: Option<u8>) {
        let mut view = ViewBuilder::new(kind, level);
        self.inline(&mut view, node, origin);
        self.blocks.extend(view.finish());
    }

    fn inline(&self, view: &mut ViewBuilder, node: &SyntaxNode, origin: Origin) {
        for element in node.children_with_tokens() {
            self.object(view, element, origin);
        }
    }

    /// Text, or an inline object.
    fn object(&self, view: &mut ViewBuilder, element: SyntaxElement, origin: Origin) {
        let (start, end) = origin.range(element.text_range());
        match element {
            SyntaxElement::Token(token) => match token.kind() {
                K::TEXT => self.push(view, start, end),
                K::WHITESPACE | K::NEW_LINE | K::BLANK_LINE => view.space(start),
                // Markers, such as a list tag's `::`.
                _ => {}
            },
            SyntaxElement::Node(object) => match object.kind() {
                K::BOLD | K::ITALIC | K::UNDERLINE | K::STRIKE => {
                    self.inline(view, &object, origin)
                }
                K::SUPERSCRIPT | K::SUBSCRIPT => self.push(view, start, end),
                K::AFFILIATED_KEYWORD => {}
                K::LINK => self.link(view, &object, origin, start),
                K::LINE_BREAK => view.space(start),
                _ => view.placeholder(start),
            },
        }
    }

    /// A link shows its description; one without, its target, which is a
    /// placeholder.
    fn link(&self, view: &mut ViewBuilder, link: &SyntaxNode, origin: Origin, at: usize) {
        let mut parts = link
            .children_with_tokens()
            .skip_while(|e| e.kind() != K::L_BRACKET);
        if parts.next().is_none() {
            view.placeholder(at);
            return;
        }
        for element in parts {
            self.object(view, element, origin);
        }
    }

    fn push(&self, view: &mut ViewBuilder, start: usize, end: usize) {
        if let Some(text) = self.source.get(start..end) {
            view.push_source(text, start);
        }
    }

    /// Code, checked only on request: a block of these ranges.
    fn code(&mut self, origin: Origin, ranges: &[TextRange]) {
        if !self.options.code_blocks {
            return;
        }
        let mut view = ViewBuilder::new(Scope::Code, None);
        for &range in ranges {
            let (start, end) = origin.range(range);
            self.push(&mut view, start, end);
        }
        self.blocks.extend(view.finish());
    }

    /// Directives in `#` comment lines.
    fn comment(&mut self, node: &SyntaxNode, origin: Origin) {
        let (start, end) = origin.range(node.text_range());
        let Some(text) = self.source.get(start..end) else {
            return;
        };
        let mut at = start;
        for line in text.split_inclusive('\n') {
            let body = line.trim_end_matches(['\n', '\r']);
            let indent = body.len() - body.trim_start().len();
            if let Some(rest) = body.trim_start().strip_prefix('#') {
                let rest_at = at + indent + 1;
                if let Some(d) = control::parse(rest, at + body.len(), |k| rest_at + k) {
                    self.directives.push(d);
                }
            }
            at += line.len();
        }
    }
}

/// A block named after an export back end, as `#+BEGIN_LaTeX` was before
/// `#+BEGIN_EXPORT latex`: not prose.
fn is_export(block: &SyntaxNode) -> bool {
    let name = child(block, K::BLOCK_BEGIN).and_then(|begin| {
        begin
            .children_with_tokens()
            .filter_map(SyntaxElement::into_token)
            .filter(|t| t.kind() == K::TEXT)
            .nth(1)
    });
    name.is_some_and(|n| {
        let n = n.text().trim().to_ascii_lowercase();
        matches!(
            n.as_str(),
            "ascii" | "beamer" | "html" | "latex" | "man" | "md" | "odt" | "texinfo"
        )
    })
}

/// `node`'s first child of `kind`.
fn child(node: &SyntaxNode, kind: K) -> Option<SyntaxNode> {
    node.children().find(|c| c.kind() == kind)
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
    fn headlines_lose_keywords_priorities_and_tags() {
        let src = "* TODO [#A] Very unique plan [1/3] :work:urgent:\n** DONE Other\n*** Plain :not tags:\n";
        assert_eq!(
            kinds(src),
            [
                (Scope::Heading, "Very unique plan #".into()),
                (Scope::Heading, "Other".into()),
                (Scope::Heading, "Plain :not tags:".into()),
            ]
        );
        let custom = "#+TODO: NEXT(n) WAIT | DONE\n* WAIT Thing\n* TODO Stays\n";
        assert_eq!(prose(custom), ["Thing", "TODO Stays"]);
    }

    #[test]
    fn heading_levels() {
        let src = "#+TITLE: Title\n#+SUBTITLE: Sub\nIntro.\n* One\n** TODO Two\nText.\n**** Four\n";
        let blocks = scan(src, Options::default()).blocks;
        assert_eq!(
            levels(&blocks),
            [
                ("Title", Some(0)),
                ("Sub", Some(0)),
                ("Intro.", None),
                ("One", Some(1)),
                ("Two", Some(2)),
                ("Text.", None),
                ("Four", Some(4)),
            ]
        );
    }

    #[test]
    fn titles_are_not_list_items() {
        let src = "#+TITLE: - A *very* title\n";
        assert_eq!(prose(src), ["- A very title"]);
        let b = &scan(src, Options::default()).blocks[0];
        let i = b.text.find("very").unwrap();
        assert_eq!(&src[b.source_range(src, i..i + 4)], "very");
    }

    #[test]
    fn comment_subtrees_are_skipped() {
        let src = "* COMMENT Draft\nutilize\n** Sub\nutilize\n* Kept\nText.\n";
        assert_eq!(prose(src), ["Kept", "Text."]);
    }

    #[test]
    fn paragraphs_and_emphasis() {
        let src = "Some *very*\n/unique/ idea, _really_ +gone+.\n\nC++ and a+b and 3*4*5 and snake_case x^2.\n";
        assert_eq!(
            prose(src),
            [
                "Some very unique idea, really gone.",
                "C++ and a+b and 3*4*5 and snake_case x^2."
            ]
        );
    }

    #[test]
    fn code_links_and_other_objects_are_placeholders() {
        let src = "Use =utilize= or ~x~, see [[https://x.org/utilize][the docs]] and [[https://x.org]].\nOn <2024-05-01 Wed> at [2024-05-01] per[fn:1] {{{m(a)}}} <<t>> @@html:<b>@@ $x$ \\(y\\) \\alpha src_py{1}.\n";
        assert_eq!(
            prose(src),
            ["Use # or #, see the docs and #. On # at # per# # # # # # # #."]
        );
    }

    #[test]
    fn spans_map_to_source() {
        let src = "A *very* unique [[x][idea]].\n";
        let block = &scan(src, Options::default()).blocks[0];
        let start = block.text.find("very unique").unwrap();
        assert_eq!(
            &src[block.source_range(src, start..start + "very unique".len())],
            "very* unique"
        );
        let idea = block.text.find("idea").unwrap();
        assert_eq!(&src[block.source_range(src, idea..idea + 4)], "idea");
    }

    #[test]
    fn lists() {
        let src = "- [ ] one\n  more of one\n  - two\n1. three\n\n   still three\n- term :: definition\nAfter.\n";
        assert_eq!(
            kinds(src),
            [
                (Scope::List, "one more of one".into()),
                (Scope::List, "two".into()),
                (Scope::List, "three".into()),
                (Scope::List, "still three".into()),
                (Scope::List, "term".into()),
                (Scope::List, "definition".into()),
                (Scope::Paragraph, "After.".into()),
            ]
        );
    }

    #[test]
    fn tables() {
        let src = "| a | b c |\n|---+-----|\n| d |   |\n";
        assert_eq!(
            kinds(src),
            [
                (Scope::Table, "a".into()),
                (Scope::Table, "b c".into()),
                (Scope::Table, "d".into()),
            ]
        );
    }

    #[test]
    fn blocks_drawers_and_keywords() {
        let src = "#+TITLE: The title\n#+AUTHOR: Me\n* H\nSCHEDULED: <2024-01-01>\n:PROPERTIES:\n:ID: x\n:END:\n#+BEGIN_SRC rust\nlet utilize = 1;\n#+END_SRC\n#+begin_quote\nQuoted.\n#+end_quote\n#+begin_verse\n  A *very*\n  unique verse\n#+end_verse\n#+BEGIN_COMMENT\nHidden.\n#+END_COMMENT\n: fixed\n-----\nEnd.\n";
        assert_eq!(
            kinds(src),
            [
                (Scope::Heading, "The title".into()),
                (Scope::Heading, "H".into()),
                (Scope::Blockquote, "Quoted.".into()),
                (Scope::Paragraph, "A very unique verse".into()),
                (Scope::Paragraph, "End.".into()),
            ]
        );
        let with_code = Options {
            code_blocks: true,
            ..Options::default()
        };
        let blocks = scan(src, with_code).blocks;
        let code: Vec<_> = blocks
            .iter()
            .filter(|b| b.kind == Scope::Code)
            .map(|b| b.text.as_str())
            .collect();
        assert_eq!(code, ["let utilize = 1;", "fixed"]);
    }

    #[test]
    fn tables_after_affiliated_keywords() {
        let src = "#+CAPTION: A table\n#+NAME: t\n| a | b c |\n\n#+NAME: p\nSome text.\n";
        assert_eq!(
            kinds(src),
            [
                (Scope::Table, "a".into()),
                (Scope::Table, "b c".into()),
                (Scope::Paragraph, "Some text.".into()),
            ]
        );
        let b = &scan(src, Options::default()).blocks[1];
        assert_eq!(&src[b.source_range(src, 0..3)], "b c");
    }

    #[test]
    fn old_export_blocks_are_skipped() {
        let src =
            "#+begin_LaTeX\n\\hypersetup{a=b}\n#+end_LaTeX\n#+begin_aside\nAside.\n#+end_aside\n";
        assert_eq!(prose(src), ["Aside."]);
    }

    #[test]
    fn unterminated_drawers_and_blocks_are_text() {
        assert_eq!(
            prose(":smile:\nHi.\n* H\n:END:\n"),
            [":smile: Hi.", "H", ":END:"]
        );
        assert_eq!(prose("#+BEGIN_SRC\nnot code\n"), ["#+BEGIN_SRC not code"]);
    }

    #[test]
    fn directives_in_comments() {
        let src = "# proserunner-disable a/b -- why\nText.\n  # plain comment\n";
        let scanned = scan(src, Options::default());
        assert_eq!(texts(&scanned.blocks), ["Text."]);
        assert_eq!(scanned.directives.len(), 1);
        let d = &scanned.directives[0];
        assert_eq!(d.action.text, "disable");
        assert_eq!(&src[d.ids[0].at..d.ids[0].at + 3], "a/b");
        assert_eq!(d.end, src.find('\n').unwrap());
    }

    #[test]
    fn footnote_definitions_are_paragraphs() {
        let src = "[fn:1] A *very* note.\n";
        assert_eq!(prose(src), ["A very note."]);
        let b = &scan(src, Options::default()).blocks[0];
        let i = b.text.find("very").unwrap();
        assert_eq!(&src[b.source_range(src, i..i + 4)], "very");
    }
}
