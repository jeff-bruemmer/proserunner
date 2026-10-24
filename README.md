# Proserunner

A fast prose linter that bends to your house style. It checks Markdown,
MDX, Org, HTML, AsciiDoc, Typst and plain text, numbers every issue, and
lets you ignore what you don't care about by number.

```console
$ proserunner document.md

document.md
[1]   3:14  "quick as lightning"  -> A tired phrase.
[2]   7:16  "software program"    -> Pleonastic phrase.
[3]  11:50  "hopefully"           -> Skunked term: consider rephrasing.
[4]  18:9   "Female booksalesman" -> Sexist or ridiculous term.
[5]  22:14  "FIX THIS"            -> Draft litter.
Found 5 issues.
```

Fix what matters, and ignore the rest by number:

```console
$ proserunner document.md --ignore-issues 2,3
```

## Install

```console
$ cargo install --path .
```

This needs Rust 1.92 or later. The result is one binary with the rules
built in. It needs no config and never uses the network.

## Use

```console
$ proserunner docs/ README.md           # Check files and directories
$ proserunner docs/ -o plain            # One issue per line, for editors and grep
$ pandoc -t markdown a.docx | proserunner -   # Check standard input (read as Markdown)
$ proserunner --stdin-filename docs/a.md < draft   # Check standard input as docs/a.md
$ proserunner explain hedging           # What a rule matches, and what changes it
$ proserunner rules                     # Every rule and its level
$ proserunner config                    # The merged config, and where each value came from
$ proserunner init                      # Start a project config in .proserunner/
```

Directories are walked in order. Hidden files, files over 10 MB, and
files listed in `.proserunnerignore`, `.ignore` or, inside a git
repository, `.gitignore`, `.git/info/exclude` or git's global excludes
file are skipped. A file you name is checked anyway, unless you pass
`--force-exclude`.

Results go to standard output. The summary, warnings and errors go to
standard error.

| Exit status | Meaning |
| --- | --- |
| `0` | No issues at `--fail-on` (default `warning`) or above |
| `1` | Issues at `--fail-on` or above |
| `2` | An error: bad flags, a missing or unsupported file, a broken config, an unknown directive |

## What it reads

| Files | Prose | Not prose |
| --- | --- | --- |
| `.md`, `.markdown` | Paragraphs, headings, list items, table cells, block quotes | Code, HTML, front matter, link targets |
| `.mdx` | What Markdown reads, and the content of JSX elements | `import` and `export`, expressions, JSX attributes, code |
| `.org` | The title, headlines, paragraphs, list items, table cells, quote blocks | Source blocks, drawers, other keywords, comments, `COMMENT` subtrees, link targets |
| `.txt` | Paragraphs | Lines between ```` ``` ```` fences |
| `.html`, `.htm`, `.xhtml` | The title, headings, paragraphs, list items, table cells, block quotes | `<pre>`, code, scripts, styles, comments, attributes |
| `.adoc`, `.asciidoc` | Titles, paragraphs, lists, table cells, quotes, admonitions, footnotes | Listing and literal blocks, comments, attributes, passthroughs, macros |
| `.typ` | The title, headings, paragraphs, list items, table cells, captions, quotes, footnotes | Raw blocks, math, comments, set, show and let rules, code |

Each format is read with an established parser for it, such as
pulldown-cmark for Markdown and the Typst compiler's own parser for
Typst. Other files can be read as one of these with `[formats]` in
config, such as `"*.qmd" = "markdown"`. Quoted text is skipped unless you
pass `--quoted-text`, and code unless you pass `--code-blocks`. Matches
are found in the text as it reads, so a phrase broken across lines, or by
`*emphasis*` or `#emph[]`, still matches. Every issue points at exact
source lines and columns. [File formats](docs/formats.md) has the
details.

## What it checks

It ships with 22 rules. `proserunner explain RULE` shows any rule's
words, patterns and examples.

| Rule | Explanation |
| --- | --- |
| `annotations` | Typical markers used to signal problem sites in the text. |
| `archaisms` | Outmoded word or phrase. |
| `cliches` | An overused phrase or opinion that betrays a lack of original thought. |
| `compression` | Compressing common phrases. From *Style: Toward Clarity and Grace* by Joseph M. Williams. |
| `corporate-speak` | Words and phrases that make you sound like an automaton. |
| `hedging` | Say, or say not. There is no hedging. |
| `jargon` | Phrases infected with bureaucracy. |
| `meta-discourse` | Self-referential phrases that add no value. |
| `needless-variants` | Prefer the more common term. |
| `nominalization` | Convert weak noun phrases back to strong verbs. |
| `non-words` | Sequences of letters masquerading as words, with a real word to use. |
| `not-the-negative` | Prefer the word to the negation of the word's opposite. |
| `overused-adverbs` | Adverbs that are weak or redundant. Consider a stronger verb instead. |
| `oxymorons` | Contradictory terms (that aren't funny). |
| `phrasal-adjectives` | Hyphenate phrasal adjectives. |
| `pompous-diction` | Use simpler words. From *Style: Toward Clarity and Grace* by Joseph M. Williams. |
| `redundancies` | Phrases that say the same thing more than once. |
| `regex` | Raw regular expressions. |
| `repetition` | A word repeated right after itself, like "the the". |
| `sexism` | Sexist or ridiculous terms (like *mail person* for *mail carrier*). |
| `skunked-terms` | Words with controversial correct usage that are best avoided. |
| `weasel-words` | Vague phrases that avoid commitment and precision. |

Each built-in rule's id is `default/<name>`, such as `default/hedging`.
The rules are pinned to the release, so results never change under you.

## Make it yours

- **Turn rules off or change their level:** see [Config](docs/config.md).

  ```toml
  # .proserunner/config.toml
  schema = 1
  fail-on = "error"
  accept = ["Kubernetes"]

  [rule."default/hedging"]
  level = "off"

  [[overrides]]
  paths = ["docs/legal/"]
  rule."default/jargon".level = "error"
  ```

- **Write your own rules:** a rule is a TOML file of words, patterns and
  terms, with examples that `proserunner rules test` checks. Checks find
  what words can't: mixed spellings, long sentences, headings in the
  wrong case, acronyms used before they're defined, and text too hard to
  read. See [Rules](docs/rules.md).

  ```toml
  # .proserunner/rules/house-style.toml  →  id "project/house-style"
  schema = 1
  message = "House style: “{suggestion}”, not “{match}”."
  words = [{ match = "e-mail", suggest = ["email"] }]

  [examples]
  bad = ["Send an e-mail."]
  good = ["Send an email."]
  ```

- **Turn rules off in a document:** with a comment in its own syntax. See
  [Inline control](docs/formats.md#inline-control).

  ```markdown
  <!-- proserunner-disable-next-line default/cliches -- quoting the brief -->
  ```

- **Ignore issues:** by number (`--ignore-issues 1,3`), every issue this
  run found (`--ignore-all`), or a word everywhere
  (`proserunner ignore add hopefully`). See [Ignores](docs/config.md#ignores).

Mistakes are never silent. A config with an unknown key or a misspelled
rule id, or a directive that names no rule, stops the run with an error
that names the file (and the line and column where there are some) and
suggests the closest match. A rule file that doesn't load is reported the
same way and left out; the other rules still run, and the run exits 2.

## Editors and CI

| `-o` | For |
| --- | --- |
| `group` | People (the default) |
| `plain` | `path:line:col: [n] "match" -> message`, for Vim's quickfix, Emacs compile mode and grep |
| `table`, `verbose` | Markdown reports |
| `json` | Programs: lines, columns and byte offsets ([format](docs/formats.md#json-output)) |
| `sarif` | Code scanning (SARIF 2.1.0) |
| `github` | GitHub Actions annotations |
| `checkstyle` | reviewdog, Jenkins and other tools that read Checkstyle XML |

For [pre-commit](https://pre-commit.com), this repository is a hook:

```yaml
- repo: https://github.com/jeff-bruemmer/proserunner
  rev: v1.0.0
  hooks:
    - id: proserunner
```

The hook passes `--force-exclude`, so files your ignore files and
`exclude` skip are skipped even though pre-commit names them.

JSON Schemas for config, rule and ignore files are in [`schemas/`](schemas),
so editors that read TOML schemas (Even Better TOML, taplo) check and
complete them. See [Config](docs/config.md#editor-support).

## Docs

- [Config](docs/config.md): layers, settings, overrides, ignores
- [Rules](docs/rules.md): writing and testing rules
- [File formats](docs/formats.md): what counts as prose, inline control, JSON and Checkstyle output
- [Upgrading from 0.8](docs/upgrading.md)
- [Development](docs/development.md): tests, the golden corpus, fuzzing, benchmarks
- [Changelog](CHANGELOG.md)
