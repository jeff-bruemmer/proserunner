# File formats

Proserunner reads each format with an established parser for it, and
never converts one format into another. It keeps the prose, drops the
markup, and remembers where every character came from, so each issue
points at exact source bytes.

The rules see each block of prose (a paragraph, a heading, a list item, a
table cell) as one line of text:

- Every run of whitespace is one space, so a phrase broken across lines
  still matches.
- Markup that only formats text, such as `*emphasis*` or `#emph[…]`,
  drops out, so `*very* unique` reads as "very unique".
- Things that print something other than prose, such as inline code, a
  link with no text, or a citation, become a placeholder. No phrase
  matches across a placeholder, and a word next to one still ends there.
- URLs (`https://…`, `www.…`, `mailto:…`) and email addresses become
  placeholders, in every format. Trailing punctuation isn't part of a
  URL.
- Quoted text becomes a placeholder, unless you pass `--quoted-text`. Curly
  double quotes (`“…”`) always quote. A straight double quote opens a
  quote only at the start of a word and closes one only at the end of a
  word, so the inch mark in `5" wide` hides nothing. Single quotes
  (`'…'`, `‘…’`) quote only with a space or punctuation outside them, so
  `it's` and `dogs'` are left alone.
- A byte order mark is skipped, and doesn't count as a column.

Code is skipped unless you pass `--code-blocks`. Then each code block is a
block of its own, in the `code` scope.

| Extension | Format | Name | Parser |
| --- | --- | --- | --- |
| `.md`, `.markdown` | Markdown | `markdown` | [pulldown-cmark](https://crates.io/crates/pulldown-cmark) |
| `.mdx` | MDX | `mdx` | [markdown](https://crates.io/crates/markdown) (markdown-rs) |
| `.org` | Org | `org` | [orgize](https://crates.io/crates/orgize) |
| `.txt` | Plain text | `text` | none: blank lines separate paragraphs |
| `.html`, `.htm`, `.xhtml` | HTML | `html` | [html5gum](https://crates.io/crates/html5gum) |
| `.adoc`, `.asciidoc` | AsciiDoc | `asciidoc` | [asciidoc-parser](https://crates.io/crates/asciidoc-parser) |
| `.typ` | Typst | `typst` | [typst-syntax](https://crates.io/crates/typst-syntax) |

The name is what `[formats]` in config and `--stdin-format` take. For
files whose extension says nothing, or the wrong thing, map a path
pattern to a format in config (see [Config](config.md#formats)):

```toml
[formats]
"*.qmd" = "markdown"
"notes/*.txt" = "org"
```

## Standard input

`-` reads standard input, as Markdown unless a flag says otherwise:

- `--stdin-format NAME` reads it as that format.
- `--stdin-filename PATH` checks it as if it were the file at `PATH`,
  which needn't exist, as with an editor's unsaved text. The format
  comes from `[formats]` and the extension, unless `--stdin-format` gives
  one. Issues name the file, its project's config applies, and so do
  `[[overrides]]` and ignore lists, and `--ignore-issues` can ignore its
  issues. With `--force-exclude`, it's skipped when a named file would
  be.
- Either flag reads standard input when no paths are given, without
  `-`. With other paths, add `-` to check standard input too.

```console
$ pandoc -t markdown report.docx | proserunner -
$ proserunner --stdin-format org < notes
$ proserunner --stdin-filename docs/guide.md < draft
```

## Markdown

CommonMark, with tables, footnotes, strikethrough, task lists, and YAML
(`---`) or TOML (`+++`) front matter.

- **Prose:** paragraphs, headings, list items, table cells (one block
  each), block quotes, footnote text, and the title and description in
  front matter (see [Front matter](#front-matter)).
- **Placeholders:** inline code, inline HTML, autolinks (`<https://…>`),
  and footnote references.
  `$…$` is text: Markdown has no math.
- **Skipped:** code blocks, HTML blocks, the rest of front matter, and
  link and image targets. A link's text is prose.

## MDX

MDX as `@mdx-js/mdx` reads it: Markdown, with the same extensions, plus
ESM, JSX and expressions.

- **Prose:** what's prose in Markdown, front matter included, and the
  content of JSX elements, such as `<Note>…</Note>`.
- **Placeholders:** inline code, images, footnote references,
  expressions in a sentence (`{props.year}`), and HTML elements that hold
  no prose, such as `<code>` and `<kbd>`.
- **Skipped:** ESM (`import` and `export`), expressions on lines of their
  own, JSX attributes, code blocks and the rest of front matter.

ESM ends where its brackets balance, as markdown-rs reads an expression
when it has no JavaScript parser. A file that isn't valid MDX, such as
one with an unclosed tag, is read as Markdown.

## Front matter

YAML between `---` lines, or TOML between `+++` lines, that opens a
Markdown or MDX file, as Jekyll and Hugo read it. A `---` anywhere else
is a thematic break. YAML is read with
[saphyr-parser](https://crates.io/crates/saphyr-parser), and TOML with
[toml](https://crates.io/crates/toml).

```yaml
---
title: Getting started      # the document's title
description: How to begin.  # a paragraph
layout: docs                # not checked
---
```

- **Prose:** the strings under `title`, `subtitle`, `description`,
  `summary`, `abstract` and `excerpt`. `title` and `subtitle` are the
  document's title (heading level 0), so heading rules check them, and
  the rest are paragraphs. Each string is a block of its own, at any
  depth under its key, such as each item of a list.
- **Choosing keys:** `check.front-matter` in config replaces the list,
  and `[]` reads no front matter. A key is a path from the top level,
  with `.` between keys: `params.summary` is the `summary` inside
  `params`, and `summary` alone is only the top-level one. Lists on the
  way are passed through, so `faq.answer` is every item's `answer`.
- **Text:** escapes are decoded (`\"`, `\u00E9`, `''`) and lines folded,
  as YAML and TOML read them, and each issue still points at the source.
  The strings are plain text: Markdown in them isn't markup.
- **Not prose:** keys, comments, and TOML's numbers, dates and booleans.
  YAML's plain `2024` under a prose key is text.
- **Mistakes:** front matter that isn't valid YAML or TOML isn't checked.
  Each parser finds front matter its own way: in Markdown, YAML front
  matter can also end with `...`, and can't start with a blank line.
- **Inline control** can't reach front matter (see
  [Inline control](#inline-control)).

## Org

- **Prose:**
  - `#+TITLE` and `#+SUBTITLE`, and headlines, are headings. A headline's
    `TODO` keyword, priority and tags aren't part of it. The keywords are
    `TODO` and `DONE`, or those a `#+TODO:` line sets.
  - Paragraphs, list items (with their checkboxes left out) and their
    tags (`- term ::`), and table cells.
  - Footnote definitions.
  - Text in `#+BEGIN_QUOTE` blocks (the `blockquote` scope) and other
    blocks such as `#+BEGIN_CENTER` and `#+BEGIN_VERSE`.
- **Inline markup:**
  - `*bold*`, `/italic/`, `_underline_` and `+strike+` keep their text.
  - `[[target][description]]` keeps its description.
  - Subscripts and superscripts keep their source, so `snake_case` stays
    one word.
  - These become placeholders: `=verbatim=` and `~code~`, a link with no
    description, timestamps, footnote references, statistics cookies,
    macros, targets, math (`$…$`, `\(…\)`, `\[…\]`), entities such as
    `\alpha`, export snippets, and inline source.
- **Skipped:**
  - Comments (`# …`), other keywords (`#+AUTHOR:` …), and drawers such as
    `:PROPERTIES:`.
  - Planning lines (`SCHEDULED:` …) and horizontal rules.
  - `COMMENT` subtrees, and comment and export blocks, including the old
    kind named after their back end, such as `#+BEGIN_LaTeX`.
  - Source and example blocks, and `:` lines: code.

A drawer or block is one only when its end line comes before the next
headline, as in Org. Otherwise its first line is plain text.

## Plain text

Paragraphs are separated by blank lines. Lines between ```` ``` ````
fences are code.

## HTML

- **Prose:**
  - Text in paragraphs, headings (`<h1>` to `<h6>`), list items, terms
    and descriptions (`list`), table cells (`table`, one block each), and
    block quotes. Other text, such as in a `<div>`, is a paragraph.
  - `<title>` is the document's title.
- **Placeholders:** `<code>`, `<kbd>`, `<samp>`, `<var>` and `<tt>`;
  images, form controls, `<svg>`, `<math>`, media and embedded objects.
- **Text:** character references are decoded as HTML decodes them, so
  `&eacute;` is é. `<q>` reads as quoted, so its text is masked like
  other quoted text.
- **Skipped:** `<pre>`, which is code; scripts, styles, templates,
  `<noscript>` and comments; and attributes, `alt` and `title` among
  them.

Proserunner closes elements where HTML implies their end, such as a `<p>`
before a block or an `<li>` before the next, but it doesn't rebuild the
tree as a browser does. As in a browser, `<script>`, `<style>`,
`<title>` and `<textarea>` run to their end tag, and a comment to its
`-->`: without one, they run to the end of the file.

## AsciiDoc

AsciiDoc as Asciidoctor reads it.

- **Prose:**
  - The document title (`=`), section titles (`==` and on, or `#`), and
    block titles (`.Title`), which are paragraphs.
  - Paragraphs, admonition paragraphs (`NOTE:` and the rest, without the
    label), and quotes (`____`, `[quote]`, `[verse]` and `>`).
  - List items, without their checkboxes, and description list terms
    and descriptions, in the `list` scope. A paragraph attached to an
    item with `+` is part of the list.
  - Table cells (`|===`, and CSV and DSV tables), one block each, except
    literal and monospace cells, which are code.
  - Example, sidebar, admonition and open blocks.
  - Footnotes: `footnote:[…]` leaves a placeholder, and its text is a
    paragraph of its own.
- **Inline markup:**
  - Bold, italic, highlight, superscript and subscript marks drop out,
    so `*very* unique` reads as "very unique". Links and cross references
    keep their text.
  - These become placeholders: monospace (`` `code` ``), passthroughs
    (`+…+`, `pass:[…]`), attribute references (`{name}`), whether the
    document defines the attribute or not, and macros such as
    `kbd:[…]`, `btn:[…]`, `menu:…[…]`, `image:…[]` and `stem:[…]`. The
    UI macros (`kbd`, `btn`, `menu`) are on unless the document unsets
    `experimental`.
  - `` "`Curved`" `` quotes are quotes, so their text is masked.
  - Typographic replacements keep what was typed: `it's`, `--` and
    `(C)` stay as they are, while `` it`'s `` reads as "it’s".
- **Skipped:**
  - Comments (`//` and `////`), the header's author and revision lines,
    attribute entries (`:name: value`), block attribute lines (`[…]`)
    and anchors.
  - Listing (`----`), literal (`....`) and `[source]` blocks, and
    indented (literal) paragraphs: code.
  - Passthrough (`++++`) and `[stem]` blocks, and block macros
    (`image::`, `toc::`).
  - Preprocessor directives (`ifdef::` … `endif::`, `include::`), which
    are read as comment lines: every branch of a conditional is checked,
    and nothing is included.

## Typst

- **Prose:**
  - Headings (`=`, `==` …, and `#heading[…]`), and `#title[…]` as the
    document's title.
  - Paragraphs, list, enum and term items (`-`, `+`, `/ Term:`), in the
    `list` scope, and the content of `#list`, `#enum` and `#terms`.
  - Table cells (`#table`), one block each, and grid cells (`#grid`) as
    paragraphs.
  - Figure captions, block quotes (`#quote(block: true)[…]`), and the
    content of layout functions such as `#block`, `#align` and `#box`.
  - Footnotes: `#footnote[…]` leaves a placeholder, and its text is a
    paragraph of its own.
- **Markup:**
  - `*strong*` and `_emphasis_` drop their markers, and the content of a
    function call, such as `#emph[…]`, `#link("…")[…]` or a function the
    document defines, is prose.
  - `#quote[…]` in a sentence reads as quoted, so its text is masked like
    other quoted text.
  - Shorthands keep what was typed (`--`, `...`), except `~`, a
    non-breaking space, and `-?`, a soft hyphen, which joins a word.
    Escapes (`\#`) are the character they escape.
  - These become placeholders: inline raw text (`` `code` ``), math
    (`$…$`), links, references (`@intro`), and calls with no content,
    such as `#image("…")`.
- **Skipped:** comments (`//`, `/* */`), labels (`<intro>`), set, show
  and let rules, imports, code, strings, and raw blocks (```` ``` ````
  over more than one line), which are code.

## Heading levels

A rule's `heading-levels` keeps it to headings at some levels (see
[Rules](rules.md#heading-levels)). Every format numbers them the same
way: 0 is a document's title, 1 its outermost sections, and each level
inside adds one.

| Format | Level 0 | Levels 1 and on |
| --- | --- | --- |
| Markdown and MDX | `title` and `subtitle` in front matter | `#` is 1, `##` 2, and so on to `######`. In Markdown, an `===` underline is 1, and `---` is 2 |
| Org | `#+TITLE` and `#+SUBTITLE` | The number of stars |
| Plain text | none | none: plain text has no headings |
| HTML | `<title>` | `<h1>` is 1, `<h2>` 2, and so on to `<h6>` |
| AsciiDoc | `= Title` or `# Title` | `==` is 1, `===` 2, and so on |
| Typst | `#title[…]` | `=` is 1, `==` 2, and so on; `#heading(level: 3)[…]` is 3 |

## Inline control

A comment can turn rules off for part of a document:

```text
proserunner-disable [ids…]             rules off from here on
proserunner-enable [ids…]              rules on again from here
proserunner-disable-next-line [ids…]   rules off on the next line only
```

| Format | Write it as |
| --- | --- |
| Markdown | `<!-- proserunner-disable default/hedging -->` |
| MDX | `{/* proserunner-disable default/hedging */}` |
| Org | `# proserunner-disable default/hedging` |
| Text | `proserunner-disable default/hedging`, on a line of its own |
| HTML | `<!-- proserunner-disable default/hedging -->` |
| AsciiDoc | `// proserunner-disable default/hedging` |
| Typst | `// proserunner-disable default/hedging` or `/* … */` |

- **Ids:** with no ids, a directive applies to every rule. Separate ids
  with spaces or commas. After `--`, the rest of the comment is a note
  for people: `<!-- proserunner-disable default/cliches -- quoting the brief -->`.
- **Position:** issues are filtered by where they start, after matching,
  so a directive works the same way for every kind of rule. An issue
  before a `proserunner-disable` on the same line is still reported.
- **Mixing:** `proserunner-enable default/hedging` after a bare
  `proserunner-disable` turns only that rule back on.
- **Next line:** `proserunner-disable-next-line` covers the line after the
  one the comment ends on, so put it directly above the line.
- **Front matter:** no directive reaches it, since nothing comes before
  it and YAML and TOML comments aren't read. Use `--ignore-issues` or
  `accept` for an issue there.
- **Mistakes:** a misspelled directive, or an id of a rule from a source
  this run loads that names no rule, stops the run with an error at the
  directive's file, line and column. So does a name without its source,
  such as `hedging`, and the error names the ids it may mean. An id from
  a source that isn't loaded, such as `project/…` checked outside the
  project, is left alone, as it is in config.

## JSON output

`-o json` prints one object:

```json
{
  "version": 1,
  "issues": [
    {
      "number": 1,
      "file": "docs/a.md",
      "line": 3,
      "column": 11,
      "end_line": 4,
      "end_column": 5,
      "byte_start": 31,
      "byte_end": 51,
      "rule": "default/compression",
      "name": "Compression",
      "level": "warning",
      "match": "due to the fact that",
      "message": "Prefer: because, since, why.",
      "suggestions": ["because", "since", "why"]
    }
  ]
}
```

- **Positions:** lines and columns start at 1. Columns count characters,
  and `end_line` and `end_column` are just past the match. `byte_start`
  and `byte_end` are exact offsets into the file.
- **`match`:** the text as the rules saw it, with whitespace collapsed.
  The source bytes can differ: `due to the fact\nthat`. A placeholder,
  such as one for inline code, is U+001A here; the other formats show
  it as `…`.
- **`number`:** what `--ignore-issues` takes.
- **`suppressed`:** with `--show-suppressed`, the issues that config,
  ignore lists and inline control hid, after `issues`. Each has no
  `number`, and has `hidden_by`, one of `directive`, `off`, `accepted` and
  `ignored`, and `reason`, which says what hid it, for people:
  `"accepted in .proserunner/config.toml"`. Without the flag, there's no
  `suppressed`.
- **`version`:** goes up only when a field changes meaning or goes away.

SARIF output counts columns in UTF-16 code units, as SARIF requires.

## Checkstyle output

`-o checkstyle` prints Checkstyle XML, for
[reviewdog](https://github.com/reviewdog/reviewdog) (`-f=checkstyle`),
Jenkins and other tools that read it:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<checkstyle version="4.3">
  <file name="docs/a.md">
    <error line="3" column="11" severity="warning" message="&quot;due to the fact that&quot; -&gt; Prefer: because, since, why." source="default/compression"/>
  </file>
  <file name="docs/clean.md"/>
</checkstyle>
```

- **Files:** every file checked has a `<file>`, with issues or without.
  A path under the working directory is relative to it, with `/`.
- **Positions:** Checkstyle has a line and a column but no end, so the
  message quotes the match. Columns count characters.
- **`severity`:** `error`, `warning`, or `info` for a hint.
- **`source`:** the rule's id.
