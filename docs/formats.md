# File formats

Proserunner reads each format with an established parser, keeps the
prose, drops the markup, and maps every character back to its source, so
each issue points at exact source bytes. The rules see each block of
prose (a paragraph, a heading, a list item, a table cell) as one line:

- Every run of whitespace is one space, so a phrase broken across lines
  still matches.
- Markup that only formats text, such as `*emphasis*` or `#emph[…]`,
  drops out, so `*very* unique` reads as "very unique".
- Inline code, URLs, email addresses, and anything else that isn't
  prose become a placeholder, which no phrase matches across.
- Quoted text becomes a placeholder, unless you pass `--quoted-text`.
  Curly double quotes always quote. Other quotes quote only at word
  edges, so `5" wide`, `it's` and `dogs'` hide nothing.
- Code is skipped, unless you pass `--code-blocks`. Then each code block
  is a block in the `code` scope.

| Extension | Format | Name | Parser |
| --- | --- | --- | --- |
| `.md`, `.markdown` | Markdown | `markdown` | [pulldown-cmark](https://crates.io/crates/pulldown-cmark) |
| `.mdx` | MDX | `mdx` | [markdown](https://crates.io/crates/markdown) (markdown-rs) |
| `.org` | Org | `org` | [orgize](https://crates.io/crates/orgize) |
| `.txt` | Plain text | `text` | none: blank lines separate paragraphs |
| `.html`, `.htm`, `.xhtml` | HTML | `html` | [html5gum](https://crates.io/crates/html5gum) |
| `.adoc`, `.asciidoc` | AsciiDoc | `asciidoc` | [asciidoc-parser](https://crates.io/crates/asciidoc-parser) |
| `.typ` | Typst | `typst` | [typst-syntax](https://crates.io/crates/typst-syntax) |

The name is what `[formats]` in [config](config.md#formats) and
`--stdin-format` take.

## Standard input

`-` reads standard input, as Markdown unless `--stdin-format NAME` says
otherwise. `--stdin-filename PATH` checks it as the file at `PATH`,
which needn't exist: its format, project, overrides and ignores apply.
Either flag reads standard input when no paths are given; with paths,
add `-`.

```console
$ pandoc -t markdown report.docx | proserunner -
$ proserunner --stdin-filename docs/guide.md < draft
```

## Markdown

CommonMark, with tables, footnotes, strikethrough, task lists, and
[front matter](#front-matter).

- **Prose:** paragraphs, headings, list items, table cells, block
  quotes, footnotes, and link text.
- **Placeholders:** inline code, inline HTML, autolinks and footnote
  references.
- **Skipped:** code blocks, HTML blocks, and link and image targets.

## MDX

Markdown, as `@mdx-js/mdx` reads it, plus ESM, JSX and expressions.

- **Prose:** what's prose in Markdown, and the content of JSX elements,
  such as `<Note>…</Note>`.
- **Placeholders:** inline code, images, footnote references,
  expressions in a sentence (`{props.year}`), and elements that hold no
  prose, such as `<code>` and `<kbd>`.
- **Skipped:** `import` and `export`, expressions on lines of their own,
  JSX attributes and code blocks.

A file that isn't valid MDX, such as one with an unclosed tag, is read
as Markdown, and so is one that markdown-rs fails on.

## Front matter

YAML between `---` lines, or TOML between `+++` lines, at the start of a
Markdown or MDX file.

```yaml
---
title: Getting started      # the document's title
description: How to begin.  # a paragraph
layout: docs                # not checked
---
```

- **Prose:** the strings under `title`, `subtitle`, `description`,
  `summary`, `abstract` and `excerpt`, at any depth under the key.
  `title` and `subtitle` are the document's title (heading level 0), and
  the rest are paragraphs. Markdown in them is plain text.
- **Choosing keys:** `check.front-matter` in config replaces the list,
  and `[]` reads none. A key is a path from the top level:
  `params.summary` is the `summary` inside `params`, and `faq.answer` is
  the `answer` in each item of a list.
- **Mistakes:** front matter that isn't valid YAML or TOML isn't
  checked.

## Org

- **Prose:** `#+TITLE`, `#+SUBTITLE` and headlines, which are headings,
  without their `TODO` keywords, priorities and tags; paragraphs, list
  items, table cells and footnotes; and text in blocks such as
  `#+BEGIN_QUOTE`.
- **Placeholders:** `=verbatim=`, `~code~`, links with no description,
  timestamps, footnote references, macros, math, entities and inline
  source.
- **Skipped:** comments, keywords (`#+AUTHOR:` …), drawers, planning
  lines, `COMMENT` subtrees, comment and export blocks, and source and
  example blocks.

## Plain text

Paragraphs are separated by blank lines. Lines between ```` ``` ````
fences are code.

## HTML

- **Prose:** `<title>`, the document's title; headings, paragraphs,
  list items, table cells and block quotes; and other text, such as in
  a `<div>`, as paragraphs. `<q>` is quoted text.
- **Placeholders:** `<code>`, `<kbd>`, `<samp>`, `<var>`, `<tt>`,
  images, form controls, `<svg>`, `<math>` and media.
- **Skipped:** `<pre>`, scripts, styles, templates, `<noscript>`,
  comments and attributes.

## AsciiDoc

AsciiDoc as Asciidoctor reads it.

- **Prose:** the document, section and block titles; paragraphs,
  admonitions, quotes, lists, table cells, and example, sidebar and open
  blocks; and footnotes, as paragraphs of their own. Links keep their
  text.
- **Placeholders:** monospace, passthroughs, attribute references
  (`{name}`), and macros such as `kbd:[…]` and `image:…[]`.
  `` "`Curved`" `` quotes are quoted text.
- **Skipped:** comments, the header's author and revision lines,
  attribute entries, block attribute lines, anchors, indented
  paragraphs, listing, literal, source, passthrough and stem blocks, and
  block macros. Every branch of
  `ifdef::` is checked, and nothing is included.

## Typst

- **Prose:** headings, `#title[…]` as the document's title, paragraphs,
  list, enum and term items, table and grid cells, figure captions,
  block quotes, and the content of any function call, such as
  `#emph[…]` or `#block[…]`. Footnotes are paragraphs of their own.
- **Placeholders:** inline raw text, math, links, references
  (`@intro`), calls with no content such as `#image("…")`, and
  `#quote[…]` in a sentence, as quoted text.
- **Skipped:** comments, labels, set, show and let rules, imports, code,
  strings and raw blocks.

## Heading levels

A rule's `heading-levels` keeps it to some levels (see
[Rules](rules.md#scope)). 0 is a document's title, 1 its outermost
sections, and each level inside adds one.

| Format | Level 0 | Levels 1 and on |
| --- | --- | --- |
| Markdown and MDX | `title` and `subtitle` in front matter | `#` is 1, `##` 2, and so on; in Markdown, an `===` underline is 1, and `---` is 2 |
| Org | `#+TITLE` and `#+SUBTITLE` | The number of stars |
| Plain text | none | none |
| HTML | `<title>` | `<h1>` is 1, `<h2>` 2, and so on |
| AsciiDoc | `= Title` or `# Title` | `==` is 1, `===` 2, and so on |
| Typst | `#title[…]` | `=` is 1, `==` 2, and so on |

## Inline control

A comment can turn rules off for part of a document:

```text
proserunner-disable [ids…]             rules off from here on
proserunner-enable [ids…]              rules on again from here
proserunner-disable-next-line [ids…]   rules off on the next line only
```

| Format | Write it as |
| --- | --- |
| Markdown, HTML | `<!-- proserunner-disable default/hedging -->` |
| MDX | `{/* proserunner-disable default/hedging */}` |
| Org | `# proserunner-disable default/hedging` |
| Text | `proserunner-disable default/hedging`, on a line of its own |
| AsciiDoc, Typst | `// proserunner-disable default/hedging` |

- **Ids:** with none, a directive applies to every rule. Separate them
  with spaces or commas. After `--`, the rest is a note for people:
  `<!-- proserunner-disable default/cliches -- quoting the brief -->`.
- **Position:** an issue is hidden by where it starts, so an issue
  before a `proserunner-disable` on the same line is still reported.
  `proserunner-disable-next-line` covers the line after the comment
  ends.
- **Front matter** comes before any directive, so use `--ignore-issues`
  or `accept` there.
- **Mistakes:** a misspelled directive or rule id stops the run, as in
  config.

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

- **Positions:** lines and columns start at 1, and columns count
  characters. The end is just past the match. Byte offsets are exact.
- **`match`:** the text as the rules saw it, with whitespace collapsed
  and a placeholder as U+001A.
- **`suppressed`:** with `--show-suppressed`, the hidden issues, each
  with no `number`, a `hidden_by` of `directive`, `off`, `accepted` or
  `ignored`, and a `reason` for people.
- **`version`** goes up only when a field changes meaning or goes away.

SARIF output counts columns in UTF-16 code units, as SARIF requires.

## Checkstyle output

`-o checkstyle` prints Checkstyle XML, for
[reviewdog](https://github.com/reviewdog/reviewdog) (`-f=checkstyle`)
and Jenkins. Every file checked has a `<file>`, issues or not. The
message quotes the match, since Checkstyle has no end position, and a
hint's `severity` is `info`.

```xml
<?xml version="1.0" encoding="UTF-8"?>
<checkstyle version="4.3">
  <file name="docs/a.md">
    <error line="3" column="11" severity="warning" message="&quot;due to the fact that&quot; -&gt; Prefer: because, since, why." source="default/compression"/>
  </file>
  <file name="docs/clean.md"/>
</checkstyle>
```
