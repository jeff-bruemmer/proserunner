# Proserunner

A fast prose linter that bends to your house style. It checks Markdown,
MDX, Org, HTML, AsciiDoc, Typst and plain text, numbers every issue, and
lets you ignore what you don't care about by number.

```console
$ proserunner document.md

document.md
[1]   3:14  warning  "quick as lightning"  -> Cliché: say it in your own words.        default/cliches
[2]   7:16  warning  "software program"    -> Redundant: “program” says it.            default/redundancies
[3]  11:50  hint     "hopefully"           -> Skunked term: rephrase to avoid it.      default/skunked-terms
[4]  18:9   warning  "Female booksalesman" -> Gendered term: use a neutral one.        default/sexism
[5]  22:14  error    "FIX THIS"            -> Draft note: resolve it, then remove it.  default/annotations
Checked 1 file and found 5 issues: 1 error, 3 warnings and 1 hint.
```

Each issue ends with its rule's id, the name that config, inline control
and `proserunner explain` take. Fix what matters, and ignore the rest by
the numbers you saw:

```console
$ proserunner document.md --ignore-issues 2,3
Ignored 2 issues in the global ignore list (~/.config/proserunner/ignores.toml), each wherever its match is in its file:
  [2] "software program" in document.md (default/redundancies)
  [3] "hopefully" in document.md (default/skunked-terms)

document.md
[1]   3:14  warning  "quick as lightning"  -> Cliché: say it in your own words.        default/cliches
[2]  18:9   warning  "Female booksalesman" -> Gendered term: use a neutral one.        default/sexism
[3]  22:14  error    "FIX THIS"            -> Draft note: resolve it, then remove it.  default/annotations
Checked 1 file and found 3 issues: 1 error and 2 warnings.
```

The numbers are the ones the last run showed, so fixing an issue first
doesn't make them name others. What's left is shown again, renumbered.

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
$ proserunner docs/ --rule hedging      # Only one rule's issues
$ proserunner docs/ --show-suppressed   # Also what config and ignores hide, and why
$ proserunner explain hedging           # What a rule matches, and what changes it
$ proserunner rules                     # Every rule and its level
$ proserunner rules set hedging off     # Change a rule's level in config
$ proserunner config                    # The merged config, and where each value came from
$ proserunner init                      # Start a project config in .proserunner/
$ proserunner guide rules               # These docs, for the version you have
```

Directories are walked in order. Hidden files, files over 10 MB, and
files listed in `.proserunnerignore`, `.ignore` or, inside a git
repository, `.gitignore`, `.git/info/exclude` or git's global excludes
file are skipped. A file you name is checked anyway, unless you pass
`--force-exclude`.

Results go to standard output. The summary, warnings and errors go to
standard error. A directory with nothing to check says so, and the
summary counts the files checked, so a run that checked nothing never
looks like a clean one. In a terminal, results are in color; `NO_COLOR`
turns it off, and `CLICOLOR_FORCE` keeps it in a pipe.

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
| `heading-periods` | A heading is a label, not a sentence, so it doesn't end with a period. |
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
| `repetition` | A word repeated right after itself, like "the the". |
| `sexism` | Sexist or ridiculous terms (like *mail person* for *mail carrier*). |
| `skunked-terms` | Words with controversial correct usage that are best avoided. |
| `weasel-words` | Vague phrases that avoid commitment and precision. |

Each built-in rule's id is `default/<name>`, such as `default/hedging`.
The rules are pinned to the release, so results never change under you.

Rules have levels. `repetition`, `annotations` and `non-words` find
near-certain mistakes, so they're errors. `overused-adverbs`, `hedging`,
`weasel-words` and `skunked-terms` give style advice, so they're hints.
The rest are warnings. Errors and warnings make a run exit 1, and hints
don't; `--fail-on error` keeps a CI check to errors. A run with many
issues ends by naming the rules and files that found the most, as a
place to start tuning: a rule to turn down, or a file that isn't prose.

## Make it yours

- **Turn rules off or change their level:** `proserunner rules set
  hedging off` writes it for you, or edit the config. `default/*` stands
  for every built-in rule. See [Config](docs/config.md).

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

- **Share a style:** a config can build on others with `extends`, and it
  gets their rules too, so a team keeps one style for every repository.
  See [Extends](docs/config.md#extends).

  ```toml
  extends = ["../house-style/config.toml"]
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

- **Ignore issues:** by the numbers the last run showed
  (`--ignore-issues 1,3`), every issue this run found (`--ignore-all`),
  or a word everywhere
  (`proserunner ignore add hopefully`). See [Ignores](docs/config.md#ignores).

- **See what's hidden:** `--show-suppressed` lists the issues that
  config, ignore lists and inline control hide, each with what hid it,
  such as "accepted in .proserunner/config.toml".

Mistakes are never silent. A config with an unknown key or a misspelled
rule id, or a directive that names no rule, stops the run with an error
that names the file (and the line and column where there are some) and
suggests the closest match. A file with several mistakes reports each
of them in one run, and a rule's name where its id goes (`hedging`) gets
the id it may mean (`default/hedging`). A rule file that doesn't load is
reported the same way and left out; the other rules still run, and the
run exits 2.

## Editors and CI

| `-o` | For |
| --- | --- |
| `group` | People (the default) |
| `plain` | `path:line:col: [n] "match" -> message (rule)`, for Vim's quickfix, Emacs compile mode and grep |
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
and `proserunner schema config` (or `rule`, or `ignores`) prints them,
so editors that read TOML schemas (Even Better TOML, taplo) check and
complete them. See [Config](docs/config.md#editor-support).

## Docs

`proserunner guide TOPIC` prints each of these for the version you
have, so they're at hand offline and in any repository.

- [Config](docs/config.md): layers, settings, extends, overrides, ignores
- [Rules](docs/rules.md): writing and testing rules
- [File formats](docs/formats.md): what counts as prose, inline control, JSON and Checkstyle output
- [Upgrading from 0.8](docs/upgrading.md)
- [Development](docs/development.md): tests, the golden corpus, fuzzing, benchmarks
- [Changelog](CHANGELOG.md)
