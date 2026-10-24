//! HTML through `html5gum`, a WHATWG tokenizer. Text in block elements is
//! prose: paragraphs, headings (`<h1>` to `<h6>`, and `<title>` as the
//! document's title), list items, table cells and block quotes. `<pre>`
//! is code. Code (`<code>`, `<kbd>`, `<samp>`, `<var>`), images, form
//! controls, SVG, math and media become placeholders. Scripts, styles and
//! templates are skipped. Character references are decoded as HTML does,
//! and `<q>` reads as quoted. Inline control directives go in `<!-- -->`
//! comments.
//!
//! The tokenizer reads tags, comments, raw text and references; this
//! module keeps a stack of open elements to know what each piece of text
//! is in. It isn't an HTML5 tree builder: it closes `<p>`, `<li>`, cells
//! and rows where HTML implies their end, and an end tag closes
//! everything opened since its start tag.

use std::convert::Infallible;

use html5gum::emitters::callback::{CallbackEmitter, CallbackEvent};
use html5gum::{Span, Tokenizer};

use super::control::{self, Directive};
use super::piece::Piece;
use super::view::ViewBuilder;
use super::{Block, Options, Scanned, Scope};

/// What an element does to the prose around and inside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    /// Starts and ends a block. Text inside takes the scope, if it has one.
    Block(Option<Scope>),
    /// A heading at this level: 0 for `<title>`.
    Heading(u8),
    /// `<pre>`: code.
    Pre,
    /// Skipped, with a placeholder where it was: `<code>`, `<svg>`.
    Placeholder,
    /// Skipped without a trace: `<script>`, `<template>`.
    Hidden,
    /// `<q>`, which a browser shows in quotation marks.
    Quote,
    /// Formatting, such as `<em>` or `<a>`, which doesn't split prose.
    Inline,
}

fn role(name: &str) -> Role {
    match name {
        "li" | "dt" | "dd" => Role::Block(Some(Scope::List)),
        "td" | "th" => Role::Block(Some(Scope::Table)),
        "blockquote" => Role::Block(Some(Scope::Blockquote)),
        "address" | "article" | "aside" | "body" | "caption" | "center" | "details" | "dialog"
        | "dir" | "div" | "dl" | "fieldset" | "figcaption" | "figure" | "footer" | "form"
        | "head" | "header" | "hgroup" | "hr" | "html" | "legend" | "main" | "menu" | "nav"
        | "ol" | "p" | "search" | "section" | "summary" | "table" | "tbody" | "tfoot" | "thead"
        | "tr" | "ul" => Role::Block(None),
        "title" => Role::Heading(0),
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
            Role::Heading(name.as_bytes()[1].saturating_sub(b'0'))
        }
        "pre" | "listing" | "plaintext" | "xmp" => Role::Pre,
        "audio" | "canvas" | "code" | "embed" | "iframe" | "img" | "input" | "kbd" | "math"
        | "object" | "samp" | "select" | "svg" | "textarea" | "tt" | "var" | "video" => {
            Role::Placeholder
        }
        "noembed" | "noframes" | "noscript" | "script" | "style" | "template" => Role::Hidden,
        "q" => Role::Quote,
        _ => Role::Inline,
    }
}

/// An element whose content isn't prose: code, media, scripts. For other
/// formats that embed HTML elements, such as MDX.
pub(super) fn is_opaque(name: &str) -> bool {
    matches!(role(name), Role::Pre | Role::Placeholder | Role::Hidden)
}

/// Code that a checked `<pre>` shows as text.
fn is_code_text(name: &str) -> bool {
    matches!(name, "code" | "kbd" | "samp" | "tt" | "var")
}

/// Elements with no end tag.
fn is_void(name: &str) -> bool {
    matches!(
        name,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "keygen"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

/// Elements deeper than this don't open. No real document nests so far,
/// and the bound keeps every look through the stack short.
const MAX_DEPTH: usize = 256;

/// A character reference is at most this long: the longest name is 32
/// bytes with its `&` and `;`.
const MAX_REFERENCE: usize = 64;

/// What the tokenizer found, with source offsets.
enum Token {
    /// A start tag: its lowercase name, where its `<` is, and whether it
    /// ends `/>`.
    Start {
        name: String,
        at: usize,
        self_closing: bool,
    },
    End {
        name: String,
        at: usize,
    },
    /// Text that starts at `at`. It ends where the next token starts.
    Text {
        at: usize,
        end: usize,
    },
    /// A comment or other markup, from `at` to `end`.
    Comment {
        at: usize,
        end: usize,
    },
    Other {
        at: usize,
    },
}

impl Token {
    fn start(&self) -> usize {
        match *self {
            Token::Start { at, .. }
            | Token::End { at, .. }
            | Token::Text { at, .. }
            | Token::Comment { at, .. }
            | Token::Other { at } => at,
        }
    }
}

/// The tokens of `source`, in order.
fn tokens(source: &str) -> Vec<Token> {
    // A start tag's name and `<`, until its `>`.
    let mut open: Option<(String, usize)> = None;
    let callback = move |event: CallbackEvent<'_>, span: Span<usize>| match event {
        CallbackEvent::OpenStartTag { name } => {
            open = Some((String::from_utf8_lossy(name).into_owned(), span.start));
            None
        }
        CallbackEvent::CloseStartTag { self_closing } => {
            let (name, at) = open.take()?;
            Some(Token::Start {
                name,
                at,
                self_closing,
            })
        }
        CallbackEvent::EndTag { name } => Some(Token::End {
            name: String::from_utf8_lossy(name).into_owned(),
            at: span.start,
        }),
        CallbackEvent::String { .. } => Some(Token::Text {
            at: span.start,
            end: span.end,
        }),
        CallbackEvent::Comment { .. } => Some(Token::Comment {
            at: span.start,
            end: span.end,
        }),
        CallbackEvent::Doctype { .. } => Some(Token::Other { at: span.start }),
        CallbackEvent::AttributeName { .. }
        | CallbackEvent::AttributeValue { .. }
        | CallbackEvent::Error(_) => None,
    };
    let mut emitter = CallbackEmitter::<_, Token, usize>::new(callback);
    emitter.naively_switch_states(true);
    Tokenizer::new_with_emitter(source, emitter)
        .map(|t: Result<Token, Infallible>| match t {
            Ok(t) => t,
            Err(never) => match never {},
        })
        .collect()
}

struct Open {
    name: String,
    role: Role,
    /// Its content isn't prose.
    skips: bool,
}

struct Scanner<'src> {
    source: &'src str,
    options: Options<'src>,
    stack: Vec<Open>,
    /// How many open elements skip their content.
    skip: usize,
    /// How many open `<pre>` elements are checked as code.
    pre: usize,
    current: Option<ViewBuilder>,
    blocks: Vec<Block>,
    directives: Vec<Directive>,
}

pub(super) fn scan(source: &str, options: Options<'_>) -> Scanned {
    let mut s = Scanner {
        source,
        options,
        stack: Vec::new(),
        skip: 0,
        pre: 0,
        current: None,
        blocks: Vec::new(),
        directives: Vec::new(),
    };
    let tokens = tokens(source);
    for (i, token) in tokens.iter().enumerate() {
        match token {
            Token::Start {
                name,
                at,
                self_closing,
            } => s.start(*at, name, *self_closing),
            Token::End { name, at } => s.end(*at, name),
            &Token::Text { at, end } => {
                // A text token ends where the next one starts. At the end of
                // the source, its own end can fall short of a reference.
                let end = match tokens.get(i + 1) {
                    Some(next) => next.start(),
                    None if !source.get(end..).is_some_and(|r| r.contains('<')) => source.len(),
                    None => end,
                };
                s.text(at, end);
            }
            &Token::Comment { at, end } => {
                if let Some(text) = source.get(at..end) {
                    control::html(&[Piece { at, text }], &mut s.directives);
                }
            }
            Token::Other { .. } => {}
        }
    }
    s.close();
    Scanned {
        blocks: s.blocks,
        directives: s.directives,
    }
}

impl Scanner<'_> {
    /// Source text `start..end` into the open block, with its character
    /// references decoded.
    fn text(&mut self, start: usize, end: usize) {
        if self.skip > 0 {
            return;
        }
        let Some(text) = self.source.get(start..end) else {
            return;
        };
        if text.trim().is_empty() {
            if let Some(view) = self.current.as_mut() {
                view.space(start);
            }
            return;
        }
        let view = self.view();
        let mut done = 0;
        for (i, _) in text.match_indices('&') {
            if i < done {
                continue;
            }
            if let Some((len, decoded)) = reference(&text[i..]) {
                view.push_source(&text[done..i], start + done);
                view.push_rewritten(&decoded, start + i..start + i + len);
                done = i + len;
            }
        }
        view.push_source(&text[done..], start + done);
    }

    fn start(&mut self, at: usize, name: &str, self_closing: bool) {
        let mut role = role(name);
        if self.pre > 0 && is_code_text(name) {
            role = Role::Inline;
        }
        if name == "br" {
            self.space(at);
            return;
        }
        let void = is_void(name);
        if self.stack.len() >= MAX_DEPTH && !void && !self_closing {
            return;
        }
        self.imply_ends(at, name, role);
        if void || self_closing {
            match role {
                _ if is_block(role) => self.close(),
                Role::Placeholder => self.placeholder(at),
                _ => {}
            }
            return;
        }
        let skips = match role {
            Role::Placeholder => {
                self.placeholder(at);
                true
            }
            Role::Hidden => true,
            Role::Pre => !self.options.code_blocks,
            _ => false,
        };
        if is_block(role) {
            self.close();
        }
        if role == Role::Quote && self.skip == 0 {
            self.view().push_rewritten("\u{201C}", at..at + 1);
        }
        if role == Role::Pre && !skips {
            self.pre += 1;
        }
        if skips {
            self.skip += 1;
        }
        self.stack.push(Open {
            name: name.to_owned(),
            role,
            skips,
        });
    }

    fn end(&mut self, at: usize, name: &str) {
        match self.stack.iter().rposition(|o| o.name == name) {
            Some(k) => self.pop_to(k, at),
            // `</p>` alone still ends a paragraph, and `</br>` is `<br>`.
            None if name == "p" => self.close(),
            None if name == "br" => self.space(at),
            None => {}
        }
    }

    /// Closes what a start tag for `name` at `at` ends, as HTML does.
    fn imply_ends(&mut self, at: usize, name: &str, role: Role) {
        let (closes, within): (&[&str], &[&str]) = match name {
            "li" => (&["li"], &["ul", "ol", "menu"]),
            "dt" | "dd" => (&["dt", "dd"], &["dl"]),
            "td" | "th" => (&["td", "th"], &["tr", "table"]),
            "tr" => (&["tr"], &["table", "thead", "tbody", "tfoot"]),
            "thead" | "tbody" | "tfoot" => (&["thead", "tbody", "tfoot"], &["table"]),
            _ => (&[], &[]),
        };
        let found = self
            .stack
            .iter()
            .rposition(|o| closes.contains(&o.name.as_str()) || within.contains(&o.name.as_str()));
        if let Some(k) = found
            && closes.contains(&self.stack[k].name.as_str())
        {
            self.pop_to(k, at);
        }
        if !is_block(role) {
            return;
        }
        // A paragraph can't hold a block, nor a heading another heading.
        if let Some(k) = self.stack.iter().rposition(|o| is_block(o.role)) {
            let open = &self.stack[k];
            let heading = matches!(role, Role::Heading(_)) && matches!(open.role, Role::Heading(_));
            if open.name == "p" || heading {
                self.pop_to(k, at);
            }
        }
    }

    /// Closes the element at `k` in the stack, and every one inside it.
    /// `at` is where its end tag is, if it has one.
    fn pop_to(&mut self, k: usize, at: usize) {
        while self.stack.len() > k {
            let Some(open) = self.stack.pop() else { break };
            if open.skips {
                self.skip -= 1;
            }
            if open.role == Role::Pre && !open.skips {
                self.pre -= 1;
            }
            if open.role == Role::Quote && self.skip == 0 {
                self.view().push_rewritten("\u{201D}", at..at + 1);
            }
            if is_block(open.role) {
                self.close();
            }
        }
    }

    fn space(&mut self, at: usize) {
        if let Some(view) = self.current.as_mut() {
            view.space(at);
        }
    }

    fn placeholder(&mut self, at: usize) {
        if self.skip == 0 {
            self.view().placeholder(at);
        }
    }

    /// The open block, opening one if needed.
    fn view(&mut self) -> &mut ViewBuilder {
        let (kind, level) = self.kind();
        self.current
            .get_or_insert_with(|| ViewBuilder::new(kind, level))
    }

    /// The innermost element's scope, and its level if it's a heading.
    fn kind(&self) -> (Scope, Option<u8>) {
        for open in self.stack.iter().rev() {
            match open.role {
                Role::Heading(level) => return (Scope::Heading, Some(level)),
                Role::Block(Some(scope)) => return (scope, None),
                Role::Pre => return (Scope::Code, None),
                _ => {}
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

fn is_block(role: Role) -> bool {
    matches!(role, Role::Block(_) | Role::Heading(_) | Role::Pre)
}

/// The character reference at the start of `text`, which starts with `&`:
/// its length and what it stands for, as the tokenizer reads it. `None`
/// when it isn't one, as in `AT&T`.
pub(super) fn reference(text: &str) -> Option<(usize, String)> {
    let b = text.as_bytes();
    let run = 1 + b[1..]
        .iter()
        .take(MAX_REFERENCE)
        .take_while(|c| c.is_ascii_alphanumeric() || **c == b'#')
        .count();
    let run = run + usize::from(b.get(run) == Some(&b';'));
    let candidate = &text[..run];
    let decoded = decode(candidate);
    // What the tokenizer didn't read as part of the reference comes after
    // it, unchanged: `&notit;` is `¬` and then `it;`.
    let same = candidate
        .bytes()
        .rev()
        .zip(decoded.bytes().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let len = run - same;
    let value = &decoded[..decoded.len() - same];
    (value != &candidate[..len]).then(|| (len, value.to_owned()))
}

/// `text`'s text as the tokenizer decodes it.
fn decode(text: &str) -> String {
    let mut out = Vec::new();
    let callback = |event: CallbackEvent<'_>, _: Span<()>| {
        if let CallbackEvent::String { value } = event {
            out.extend_from_slice(value);
        }
        None::<Infallible>
    };
    Tokenizer::new_with_emitter(text, CallbackEmitter::new(callback)).for_each(drop);
    String::from_utf8_lossy(&out).into_owned()
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
    fn block_kinds_and_heading_levels() {
        let src = "<!DOCTYPE html>\n<html><head><title>The &amp; title</title>\n\
            <meta charset=utf-8><style>p { color: red }</style></head>\n\
            <body><h1>One</h1><p>Text <em>very</em> unique.</p>\n\
            <ul><li>first<li>second</ul><blockquote><p>quoted</p></blockquote>\n\
            <table><tr><th>a<td>b</table><h3>Three</h3><dl><dt>term<dd>meaning</dl></body></html>";
        let blocks = scan(src, Options::default()).blocks;
        assert_eq!(
            levels(&blocks),
            [
                ("The & title", Some(0)),
                ("One", Some(1)),
                ("Text very unique.", None),
                ("first", None),
                ("second", None),
                ("quoted", None),
                ("a", None),
                ("b", None),
                ("Three", Some(3)),
                ("term", None),
                ("meaning", None),
            ]
        );
        let scopes: Vec<_> = blocks.iter().map(|b| b.kind).collect();
        assert_eq!(
            scopes,
            [
                Scope::Heading,
                Scope::Heading,
                Scope::Paragraph,
                Scope::List,
                Scope::List,
                Scope::Blockquote,
                Scope::Table,
                Scope::Table,
                Scope::Heading,
                Scope::List,
                Scope::List,
            ]
        );
    }

    #[test]
    fn implied_paragraph_ends() {
        assert_eq!(
            prose("<p>one<p>two<div>three</div>four"),
            ["one", "two", "three", "four"]
        );
        assert_eq!(prose("<h1>a<h2>b</h2>c"), ["a", "b", "c"]);
    }

    #[test]
    fn code_media_and_scripts() {
        let src = "<p>Run <code>utilize()</code> or <kbd>Ctrl</kbd>, see <img src=x alt=y> and \
                   <svg><text>utilize</text></svg>.</p><script>if (a < b) utilize()</script>\
                   <template><p>utilize</p></template><pre>utilize</pre>\
                   <noscript><p>utilize</p></noscript><p>Type <textarea>utilize</textarea> here.";
        assert_eq!(prose(src), ["Run # or #, see # and #.", "Type # here."]);
        let options = Options {
            code_blocks: true,
            ..Options::default()
        };
        let blocks = scan("<pre><code>let x = 1;</code></pre>", options).blocks;
        assert_eq!(texts(&blocks), ["let x = 1;"]);
        assert_eq!(blocks[0].kind, Scope::Code);
    }

    #[test]
    fn character_references() {
        let src = "<p>caf&eacute; &mdash; &#233;t&#xE9; &Sigma;&sigma; a&nbsp;b &bogus; &notit; Q&A, AT&T &amp</p>";
        assert_eq!(prose(src), ["café — été Σσ a b &bogus; ¬it; Q&A, AT&T &"]);
        let b = &scan(src, Options::default()).blocks[0];
        let i = b.text.find("été").unwrap();
        assert_eq!(
            &src[b.source_range(src, i..i + "été".len())],
            "&#233;t&#xE9;"
        );
        let i = b.text.find("it;").unwrap();
        assert_eq!(&src[b.source_range(src, i..i + 3)], "it;");
    }

    #[test]
    fn a_reference_at_the_end_is_whole() {
        let src = "a &eacute;";
        let b = &scan(src, Options::default()).blocks[0];
        assert_eq!(b.text, "a é");
        assert_eq!(&src[b.source_range(src, 2..4)], "&eacute;");
    }

    #[test]
    fn attributes_may_hold_markup_characters() {
        let src = "<p title=\"a > b\" data-x='<p>'>Text</p><a href=\"x>Unclosed</a>";
        assert_eq!(prose(src), ["Text"]);
    }

    #[test]
    fn inline_quotes_are_quoted_text() {
        let src = "<p>She said <q>obviously</q> no.</p>";
        let blocks = scan(src, Options::default()).blocks;
        assert_eq!(texts(&blocks), ["She said “obviously” no."]);
        let b = &blocks[0];
        let i = b.text.find("obviously").unwrap();
        assert_eq!(&src[b.source_range(src, i..i + 9)], "obviously");
    }

    #[test]
    fn unclosed_raw_text_and_comments_run_to_the_end() {
        assert_eq!(prose("<p>a<script>b</p><p>c"), ["a"]);
        assert_eq!(prose("<p>a <!-- b</p>"), ["a"]);
        assert_eq!(prose("<p>a < b</p>"), ["a < b"]);
    }

    #[test]
    fn line_endings_and_multibyte_text() {
        let src = "<p>naïve\r\ncafé</p>";
        let b = &scan(src, Options::default()).blocks[0];
        assert_eq!(b.text, "naïve café");
        let i = b.text.find("café").unwrap();
        assert_eq!(&src[b.source_range(src, i..b.text.len())], "café");
    }

    #[test]
    fn spans_map_to_source() {
        let src = "<p>Some <b>very</b> unique text.</p>";
        let b = &scan(src, Options::default()).blocks[0];
        let i = b.text.find("very unique").unwrap();
        assert_eq!(&src[b.source_range(src, i..i + 11)], "very</b> unique");
    }

    #[test]
    fn directives_in_comments() {
        let src = "<!-- proserunner-disable a/b -->\n<p>Text <!-- proserunner-enable --> here.</p>\n<script><!-- proserunner-disable --></script>";
        let scanned = scan(src, Options::default());
        let found: Vec<_> = scanned
            .directives
            .iter()
            .map(|d| d.action.text.as_str())
            .collect();
        assert_eq!(found, ["disable", "enable"]);
        assert_eq!(&src[scanned.directives[0].ids[0].at..][..3], "a/b");
        assert_eq!(texts(&scanned.blocks), ["Text here."]);
    }
}
