# Proserunner

A fast prose linter that bends to your house style. It checks Markdown,
Org, TeX and plain text, numbers every issue, and lets you ignore what you
don't care about by number.

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

This needs Rust 1.89 or later. The result is one binary with the rules
built in. It needs no config and never uses the network.

## Use

```console
$ proserunner docs/ README.md           # Check files and directories
$ proserunner docs/ -o plain            # One issue per line, for editors and grep
$ pandoc -t markdown a.docx | proserunner -   # Check standard input (read as Markdown)
$ proserunner explain hedging           # What a rule matches, and what changes it
$ proserunner rules                     # Every rule and its level
$ proserunner config                    # The merged config, and where each value came from
$ proserunner init                      # Start a project config in .proserunner/
```

Directories are walked in order. Hidden files, files over 10 MB, and
files that `.gitignore` (inside a git repository), `.ignore` or
`.proserunnerignore` list are skipped. A file you name is always checked.

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
| `.org` | The title, headlines, paragraphs, list items, table cells, quote blocks | Source blocks, drawers, other keywords, comments, `COMMENT` subtrees, link targets |
| `.tex` | The title, section titles, paragraphs, items, table cells, footnotes, captions | The rest of the preamble, comments, math, verbatim, labels, references, citations |
| `.txt` | Paragraphs | Lines between ```` ``` ```` fences |

Quoted text is skipped unless you pass `--quoted-text`, and code unless
you pass `--code-blocks`. Matches are found in the text as it reads, so a
phrase broken across lines, or by `*emphasis*` or `\emph{}`, still
matches. Every issue points at exact source lines and columns.
[File formats](docs/formats.md) has the details.

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

- **Write your own rules:** a rule is a TOML file of words and patterns,
  with examples that `proserunner rules test` checks. See
  [Rules](docs/rules.md).

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
- [File formats](docs/formats.md): what counts as prose, inline control, JSON output
- [Upgrading from 0.8](docs/upgrading.md)
- [Development](docs/development.md): tests, the golden corpus, fuzzing, benchmarks
- [Changelog](CHANGELOG.md)
# Config

Proserunner works with no config at all. When you want one, it's TOML,
in up to three layers:

| Layer | File |
| --- | --- |
| User | `~/.config/proserunner/config.toml` (or `$XDG_CONFIG_HOME/proserunner/`) |
| Project | `.proserunner/config.toml` in the project root |
| Explicit | the file given with `--config FILE` |

The project is the nearest directory, from the files you check up, that
has a `.proserunner/` directory, so `proserunner ~/work/site/docs` uses
that site's config from anywhere. With only standard input to check, and
for commands that check nothing (`ignore`, `rules`, `config`), the search
starts at the working directory. It stops at your home directory and at
the root of a git repository. Files in two different projects can't be
checked in one run, since a run has one config. `proserunner init`
starts a project in the working directory.

`proserunner config` prints the merged config, with the file each value
came from:

```console
$ proserunner config
# Merged from: ~/.config/proserunner/config.toml, .proserunner/config.toml
schema = 1
rules = ["default", { path = ".proserunner/rules", prefix = "project" }]  # .proserunner/config.toml
fail-on = "warning"  # built-in default
accept = [
    "Kubernetes",  # ~/.config/proserunner/config.toml
]
…
```

## An example

```toml
schema  = 1
rules   = ["default", { path = ".proserunner/rules", prefix = "project" }]
fail-on = "warning"
accept  = ["Kubernetes"]          # never reported by any rule
mask    = ['\{\{<.*?>\}\}']       # Hugo shortcodes: no rule sees them

[check]
exclude     = ["drafts/", "*.draft.md"]
quoted-text = false
code-blocks = false
output      = "group"

[rule."default/hedging"]
level = "off"

[rule."default/jargon"]
accept = ["synergy"]              # never reported by this rule

[[overrides]]
paths = ["docs/legal/"]
accept = ["hereinafter"]
rule."default/hedging".level = "error"
```

## Settings

| Key | Default | Meaning |
| --- | --- | --- |
| `schema` | (required) | The format version: `1` |
| `inherit` | `true` | `false` leaves out the layers below this one, and their ignore lists |
| `rules` | `["default"]` | Where rules come from (see [Rules](rules.md#where-rules-come-from)) |
| `fail-on` | `"warning"` | The least serious level that makes a run exit 1: `"hint"`, `"warning"` or `"error"` |
| `accept` | none | Matches no rule reports |
| `mask` | none | Regular expressions for text no rule should see |
| `check.exclude` | none | Files to skip, as [path patterns](#path-patterns) |
| `check.quoted-text` | `false` | Check quoted text too |
| `check.code-blocks` | `false` | Check code blocks too |
| `check.output` | `"group"` | The output format |
| `rule."ID".level` | the rule's own | `"off"`, `"hint"`, `"warning"` or `"error"` |
| `rule."ID".accept` | none | Matches this rule doesn't report |
| `overrides` | none | Settings for some files (see [Overrides](#overrides)) |

Command-line options win over `check.*`: `--quoted-text` and
`--no-quoted-text`, `--code-blocks` and `--no-code-blocks`, `-o`,
`--fail-on`, and `--exclude`, which adds to `check.exclude`.

- **`accept`:** an entry is compared with a whole match, in any case and
  with whitespace collapsed, never with part of a match or as a pattern.
  So `accept = ["synergy"]` keeps "Synergy" from being reported, but not
  "synergies".
- **`mask`:** patterns run on each block's prose, after markup is gone,
  and what they match becomes placeholders, like inline code. Use it for
  template syntax and the like.
- **Rule ids:** a rule id in `[rule."…"]` must name a rule. A misspelled
  one stops the run, with a "did you mean" when a rule's id is close. An
  id from a rule source this run doesn't load, such as a project's rules
  in your user config, is left alone, so one config can serve several
  projects, unless it's a typo away from a loaded rule
  (`defualt/hedging`), which stops the run too.

## How layers merge

Layers apply in order: built-in defaults, user, project, then `--config`.

- **Tables** (`[check]`, `[rule."…"]`) merge key by key.
- **Values and lists** from a later layer replace earlier ones, except
  `accept`, `mask` and `check.exclude`, which add to them.
- **`[[overrides]]`** add up too, and apply in order.
- **`inherit = false`** in a project config leaves out the user config
  and the global ignore list. In a `--config` file, it leaves out both
  the user and the project config, and every ignore list.

Paths in a project config are relative to the project root. Paths in any
other config are relative to the directory that holds it. `~/` is your
home directory.

## Overrides

```toml
[[overrides]]
paths = ["docs/legal/", "*.contract.md"]
accept = ["hereinafter"]
rule."default/hedging".level = "error"
rule."default/jargon".accept = ["party of the first part"]
```

An override applies to the files its `paths` match. It can set `accept`,
and a `level` or `accept` for any rule. When several match a file, they
apply in order, so the last one wins.

## Path patterns

`check.exclude` and `overrides.paths` use gitignore's rules:

| Pattern | Matches |
| --- | --- |
| `*.draft.md` | at any depth (no `/`) |
| `drafts/` | a directory named `drafts`, at any depth, and everything in it |
| `docs/legal` | `docs/legal` under the base directory, and everything in it |
| `/docs/*.md` | Markdown files directly in `docs/` under the base directory |
| `~/notes/**` | everything under `~/notes` |

The base directory is the project root, or the directory holding the
config, and a leading `../` climbs from it. `*` doesn't cross a `/`, and
`**` does. Negated patterns (`!…`) aren't supported.

Files can also be skipped with a `.proserunnerignore` file (gitignore
syntax), anywhere in the tree, or with `--exclude PATTERN`, which is
relative to each directory you check. Repeat `--exclude` for more than
one pattern.

A file you name is checked even when one of these would skip it, unless
you pass `--force-exclude`. Hooks that name every changed file, such as
pre-commit and lint-staged, need it; this repository's
`.pre-commit-hooks.yaml` passes it.

## Ignores

Ignores are what proserunner writes for you, kept apart from config in
`ignores.toml`. There's one global list, in
`~/.config/proserunner/ignores.toml`, and one per project, in
`.proserunner/ignores.toml`.

```console
$ proserunner document.md --ignore-issues 1,3-5   # These issues, by number
$ proserunner document.md --ignore-all            # Every issue this run found
$ proserunner ignore add hopefully "very unique"  # These words, everywhere
$ proserunner ignore remove hopefully
$ proserunner ignore list
$ proserunner ignore audit     # Ignores for files that no longer exist
$ proserunner ignore clean     # Remove them
$ proserunner ignore clear     # Remove every ignore (asks first)
$ proserunner document.md --no-ignores            # A run without ignores
```

- **Which list:** commands use the project's list inside a project, and
  the global one outside. `--global` or `--project` chooses.
- **Numbered issues:** an issue ignored by number is ignored in that file
  for that rule wherever the same words appear. Edits that move it don't
  bring it back.
- **Matching:** words are compared like `accept`: whole matches, any
  case.
- **Comments:** proserunner keeps the comments you add to an
  `ignores.toml`. A file that doesn't parse is never overwritten: the
  command stops with an error instead.
- **`clear` in scripts:** outside a terminal, `ignore clear` needs
  `--force`.

```toml
schema = 1
accept = ["hopefully"]

[[issue]]
file = "docs/api.md"             # relative to the project root
rule = "default/pompous-diction" # leave it out for every rule
match = "utilize"
line = 42                        # optional: only on this line
```

## Editor support

The JSON Schemas in [`schemas/`](../schemas) describe each file:

| File | Schema |
| --- | --- |
| `config.toml` | `schemas/config.schema.json` |
| rule files | `schemas/rule.schema.json` |
| `ignores.toml` | `schemas/ignores.schema.json` |

Editors that read TOML schemas, such as VS Code with Even Better TOML,
and anything else built on taplo, then complete keys, show what each one
means, and mark mistakes as you type. Copy the schema next to the file
and name it on the first line:

```toml
#:schema ./config.schema.json
schema = 1
```

Or copy `schemas/` into the project and tell taplo once, in a
`.taplo.toml` at the project root:

```toml
[[rule]]
include = ["**/.proserunner/config.toml"]
schema.path = "./schemas/config.schema.json"

[[rule]]
include = ["**/.proserunner/rules/*.toml"]
schema.path = "./schemas/rule.schema.json"
```
# Development

## How it fits together

```text
walk → read → scan → lint → policy → report → format
 I/O    I/O   pure   pure    pure      pure     I/O
```

| Module | Does |
| --- | --- |
| `walk` | Finds the files to check |
| `scan` | Turns a document into prose blocks and inline control directives, one scanner per format |
| `rules` | Reads rule files and compiles them into one matcher for all of them |
| `lint` | Matches every rule against each block in one pass |
| `policy` | Applies inline control, levels, `accept`, overrides and ignores to each file's issues |
| `report` | Orders and numbers the issues |
| `format` | Writes them in each output format |
| `config`, `ignores`, `ctx` | Settings, and where they live |
| `migrate` | Converts 0.8's EDN files |

`scan`, `lint`, `policy` and `report` do no I/O, so other tools can use
them as a library. Errors are one `Error` enum, and only `main` turns them
into messages and exit statuses.

**Scanners** build each block with `ViewBuilder`. It collapses
whitespace, drops markup, turns non-prose into placeholders, and keeps a
map from each byte of the block back to the source. A scanner returns
`Scanned { blocks, directives }`. It reads directives from its own comments
with `control::parse`.

**Matching:**

- `words` from every rule go into one Aho-Corasick automaton per case
  mode.
- `patterns` go into one `RegexSet`, and a pattern runs only in blocks
  where the set says it matches.
- Patterns that need lookaround run on `fancy-regex`, with a backtracking
  budget.
- Word edges are checked after matching, so the automata stay on their
  fast paths.

## Checks

```console
$ cargo test
$ cargo clippy --all-targets -- -D warnings
$ cargo fmt --check
```

`unwrap`, `expect` and `panic!` are denied outside tests. The tests are
hermetic: each black-box test gets its own home and config directory.

To run the binary by hand, give it a scratch home too. Every command
first converts any 0.8 settings it finds, and with your real `HOME` that
means your own `~/.config/proserunner/`:

```console
$ HOME=/tmp/pr XDG_CONFIG_HOME=/tmp/pr/.config target/debug/proserunner docs/
```

## The golden corpus

`tests/golden.rs` snapshots what the built-in rules find in `resources/`:
Markdown, Org and TeX samples. A change to matching or scanning shows up
there as a snapshot diff. Review it with `cargo insta review`. A changed
snapshot needs a "Behaviour changes" entry in the [changelog](../CHANGELOG.md).

## JSON Schemas

`schemas/*.schema.json` are generated from the types that read each file.
The derives exist only in tests, so `schemars` isn't a dependency of the
binary. A test fails when the files are out of date. To rewrite them:

```console
$ PROSERUNNER_WRITE_SCHEMAS=1 cargo test json_schemas
```

Doc comments on those types become the descriptions editors show, so
write them for people editing the files.

## Fuzzing

`fuzz/` holds [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz)
targets:

| Target | Input | Checks |
| --- | --- | --- |
| `scan` | A format byte, then a document | Every scanner, lint and output format runs without panicking, and every span lies on character boundaries |
| `rules` | A rule file | The compiler accepts or rejects it, and a compiled rule lints, explains and runs its examples |
| `edn` | A 0.8 check file | The EDN reader and the check converter accept or reject it |

With a nightly toolchain:

```console
$ cargo +nightly fuzz run scan
```

On stable, build them with the flags `cargo fuzz` would pass (without the
sanitizers, which need nightly), then run a target on its corpus:

```console
$ cd fuzz
$ RUSTFLAGS="-Cpasses=sancov-module -Cllvm-args=-sanitizer-coverage-level=4 \
    -Cllvm-args=-sanitizer-coverage-inline-8bit-counters \
    -Cllvm-args=-sanitizer-coverage-pc-table \
    -Cllvm-args=-sanitizer-coverage-trace-compares --cfg fuzzing \
    -Cdebug-assertions -Coverflow-checks" \
  cargo build --release --bins --target x86_64-unknown-linux-gnu
$ mkdir -p corpus/scan corpus/rules
$ target/x86_64-unknown-linux-gnu/release/scan corpus/scan seeds/scan -max_total_time=600
$ target/x86_64-unknown-linux-gnu/release/rules corpus/rules ../defaults -max_total_time=600
```

`seeds/scan/` holds the samples in `resources/`, each after a format byte
(`0` Markdown, `1` Org, `2` TeX, `3` text; add `4` for quoted text and
`8` for code blocks). `corpus/` is what a run grows, and isn't kept. When
a target finds a crash, add the input as a regular test beside the code
it broke, replay `artifacts/` against the fix (pass the files instead of
a directory), and then delete them.

## Speed

```console
$ cargo bench
```

The targets are 5 ms to lint 5,000 lines (`lint 5k lines`), and 10 ms for
a whole run on a 2 KB file. Compiling the built-in rules is a fixed cost
of every run.

## Adding things

- **A built-in rule:** add a file to `defaults/`. `build.rs` embeds every
  file there. Give it `bad` and `good` examples: `cargo test` runs them,
  and the golden snapshot shows what it finds.
- **A file format:** add a scanner in `src/scan/` that returns
  `Scanned`, map its extension in `Format::of`, and add it to `FORMATS`
  in `fuzz/fuzz_targets/scan.rs`, then to the golden corpus.
- **An output format:** add a module in `src/format/` and a variant of
  `Output`. Every format gets the same numbered `Report`.
# File formats

Proserunner reads each format with its own scanner and never converts one
format into another. A scanner keeps the prose, drops the markup, and
remembers where every character came from, so each issue points at exact
source bytes.

The rules see each block of prose (a paragraph, a heading, a list item, a
table cell) as one line of text:

- Every run of whitespace is one space, so a phrase broken across lines
  still matches.
- Markup that only formats text, such as `*emphasis*` or `\emph{…}`,
  drops out, so `*very* unique` reads as "very unique".
- Things that print something other than prose, such as inline code, a
  link with no text, or a citation, become a placeholder. No phrase
  matches across a placeholder, and a word next to one still ends there.
- URLs (`https://…`, `www.…`, `mailto:…`) and email addresses become
  placeholders, in every format. Trailing punctuation isn't part of a
  URL.
- Quoted text becomes spaces, unless you pass `--quoted-text`. Curly
  double quotes (`“…”`) always quote. A straight double quote opens a
  quote only at the start of a word and closes one only at the end of a
  word, so the inch mark in `5" wide` hides nothing. Single quotes
  (`'…'`, `‘…’`) quote only with a space or punctuation outside them, so
  `it's` and `dogs'` are left alone.
- A byte order mark is skipped, and doesn't count as a column.

Code is skipped unless you pass `--code-blocks`. Then each code block is a
block of its own, in the `code` scope.

| Extension | Format |
| --- | --- |
| `.md`, `.markdown` | Markdown |
| `.org` | Org |
| `.tex` | TeX and LaTeX |
| `.txt` | Plain text |

Standard input (`-`) is read as Markdown. Other formats can be converted
and piped in: `pandoc -t markdown report.docx | proserunner -`.

## Markdown

CommonMark, with tables, footnotes, strikethrough, task lists, and YAML
(`---`) or TOML (`+++`) front matter.

- **Prose:** paragraphs, headings, list items, table cells (one block
  each), block quotes, and footnote text.
- **Placeholders:** inline code, inline HTML, autolinks (`<https://…>`),
  and footnote references.
  `$…$` is text: Markdown has no math.
- **Skipped:** code blocks, HTML blocks, front matter, and link and image
  targets. A link's text is prose.

## Org

- **Prose:**
  - `#+TITLE` and `#+SUBTITLE`, and headlines, are headings. A headline's
    `TODO` keyword, priority and tags aren't part of it. The keywords are
    `TODO` and `DONE`, or those a `#+TODO:` line sets.
  - Paragraphs, list items (with their checkboxes left out), and table
    cells.
  - Footnote definitions.
  - Text in `#+BEGIN_QUOTE` blocks (the `blockquote` scope) and other
    blocks such as `#+BEGIN_CENTER` and `#+BEGIN_VERSE`.
- **Inline markup:**
  - `*bold*`, `/italic/`, `_underline_` and `+strike+` keep their text.
  - `[[target][description]]` keeps its description.
  - These become placeholders: `=verbatim=` and `~code~`, a link with no
    description, timestamps, footnote references, statistics cookies,
    macros, targets, math (`$…$`, `\(…\)`, `\[…\]`), entities such as
    `\alpha`, export snippets, and inline source.
- **Skipped:**
  - Comments (`# …`), other keywords (`#+AUTHOR:` …), and drawers such as
    `:PROPERTIES:`.
  - Planning lines (`SCHEDULED:` …) and horizontal rules.
  - `COMMENT` subtrees, and comment and export blocks.
  - Source and example blocks, and `:` lines: code.

A drawer or block is one only when its end line comes before the next
headline, as in Org. Otherwise its first line is plain text.

## TeX

- **Prose:**
  - Everything after `\begin{document}`, or the whole file when there's
    no `\begin{document}`, as in a chapter that's `\input` elsewhere.
    Nothing after `\end{document}`.
  - `\title{…}` and sectioning commands (`\section{…}` and the rest) are
    headings, even in the preamble.
  - `\footnote{…}`, `\caption{…}` and `\marginpar{…}` are paragraphs of
    their own. A footnote leaves a placeholder where it was.
  - `itemize`, `enumerate` and `description` items are list items.
    `\item[…]` labels are part of their item.
  - `quote`, `quotation` and `verse` are block quotes. In `tabular` and
    the like, each cell is a block.
- **Commands** are read by what they print:
  - Formatting commands drop out and keep their argument:
    `\emph{very} unique` reads as "very unique". So do commands
    proserunner doesn't know, when they have an argument.
  - Commands that print something other than prose become a placeholder:
    `\ref`, `\cite` and its variants, `\url`, `\texttt`, glossary and
    unit commands. So do commands proserunner doesn't know that have no
    argument, such as `\LaTeX` and `\today`.
  - Commands that print nothing drop out with their arguments: `\label`,
    `\index`, `\includegraphics`, `\vspace`, `\newcommand`, `\noindent`
    and the like. `\href{url}{text}` keeps only its text.
  - As in TeX, the spaces after a command word don't count, so
    `\LaTeX is` reads as "LaTeXis". Write `\LaTeX{} is` or `\LaTeX\ is`.
  - A comment swallows its line ending, so `very%` at the end of a line,
    followed by `unique`, reads as "veryunique".
- **Text:**
  - ``` ``quotes'' ``` and `` `quotes' `` read as `"quotes"` and
    `'quotes'`, so they're quoted text.
  - `~` is a space, `\%` is `%`, and accents such as `\'e` are letters
    (é).
- **Skipped:**
  - The preamble, apart from the title.
  - Comments, math (`$…$`, `$$…$$`, `\(…\)`, `\[…\]`, and environments
    such as `equation` and `align`), and `\verb|…|`.
  - `verbatim`, `lstlisting` and `minted` environments, which are code.
  - `comment`, `tikzpicture` and `thebibliography` environments.

## Plain text

Paragraphs are separated by blank lines. Lines between ```` ``` ````
fences are code.

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
| Org | `# proserunner-disable default/hedging` |
| TeX | `% proserunner-disable default/hedging` |
| Text | `proserunner-disable default/hedging`, on a line of its own |

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
- **Mistakes:** a misspelled directive, or an id of a rule from a source
  this run loads that names no rule, stops the run with an error at the
  directive's file, line and column. An id from a source that isn't
  loaded, such as `project/…` checked outside the project, is left alone,
  as it is in config.

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
  The source bytes can differ: `due to the fact\nthat`.
- **`number`:** what `--ignore-issues` takes.
- **`version`:** goes up only when a field changes meaning or goes away.

SARIF output counts columns in UTF-16 code units, as SARIF requires.
# Rules

A rule is a TOML file of words and patterns to report, with a message and
examples. Its id is `<source>/<file name>`: the built-in `hedging.toml` is
`default/hedging`.

```toml
# .proserunner/rules/house-style.toml  →  id "project/house-style"
schema  = 1
message = "House style: “{suggestion}”, not “{match}”."
level   = "warning"
explain = "Our style guide, section 3."
scope   = ["paragraph", "list", "heading"]

words = [
  { match = "synerg{y,ies}", message = "Say what “{match}” means." },
  { match = "e-mail", suggest = ["email"] },
  { match = "utilize", suggest = ["use"], message = "Say “use”." },
]
patterns = [
  { match = '!{2,}', message = "One exclamation mark is plenty." },
]

[examples]
bad  = ["Send an e-mail.", "Our synergies grew.", "Wow!!"]
good = ["Send an email.", "Wow!"]
fix  = [["Send an e-mail.", "Send an email."]]
```

## Where rules come from

The config's `rules` list names the sources. `"default"` is the built-in
rules. Anything else is a directory, and every `*.toml` file directly in
it is a rule:

```toml
rules = ["default", "~/writing/rules", { path = ".proserunner/rules", prefix = "project" }]
```

A directory's rules get ids that start with the directory's name, or
with `prefix` when you give one. `proserunner init` sets up
`.proserunner/rules/` with the prefix `project`. To start from the
built-ins, `proserunner rules export DIR` copies them into `DIR`.

## Keys

| Key | Default | Meaning |
| --- | --- | --- |
| `schema` | (required) | The format version: `1` |
| `name` | from the file name | The name people see: `needless-variants.toml` is "Needless variants" |
| `message` | none | The message for each issue (see [Messages](#messages)) |
| `level` | `"warning"` | `"hint"`, `"warning"` or `"error"` |
| `explain` | none | Why the rule exists, for `explain` and verbose output |
| `case` | `"insensitive"` | `"insensitive"` or `"sensitive"` |
| `scope` | everywhere | Where the rule applies (see [Scope](#scope)) |
| `except` | none | Matches never to report |
| `words` | none | Literal words and phrases |
| `patterns` | none | Regular expressions |
| `repeat` | `false` | Report a word repeated right after itself |
| `examples` | none | `bad`, `good` and `fix` text for `rules test` |

A rule needs at least one of `words`, `patterns` or `repeat = true`. An
unknown key, an unknown `scope` or `level`, an invalid pattern, or a
message that can't be filled in is an error that names the file (and the
line and column, for a TOML mistake). A rule file with an error is left
out of the run with a warning, and the other rules still run.

## Words

A `words` entry is a literal string. Nothing in it is a regular
expression, so `etc.` and `C++` mean exactly what they say.

- **Case:** with `case = "insensitive"`, any case matches, with Unicode
  case folding, so `über` matches `Über`.
- **Spaces:** a space matches any run of whitespace, line breaks
  included.
- **Word edges:** a word that starts with a letter, digit or `_` matches
  only at the start of a word, and one that ends with one matches only at
  the end of a word. So `leverage` doesn't match `deleverage`, and `etc.`
  matches before any character. A word doesn't start right after a
  hyphen, so `known` doesn't match in `well-known`.
- **Braces:** `{a,b}` expands into each choice: `synerg{y,ies}` is
  `synergy` and `synergies`, and `thought leader{,s}` is
  `thought leader` and `thought leaders`. Braces without a comma are
  literal, and `\{`, `\}`, `\,` and `\\` escape. An entry can expand into
  1,000 words at most, and braces can't nest.

## Patterns

A `patterns` entry is a regular expression in
[Rust's syntax](https://docs.rs/regex/latest/regex/#syntax), used exactly
as written. Nothing is added, so write `\b` where you want a word
boundary. With `case = "insensitive"`, `(?i)` goes in front.

Patterns match the text as the rules see it (see
[File formats](formats.md)): markup is gone, whitespace is one space, and
inline code is a placeholder (U+001A).

Lookaround and backreferences work. Patterns that use them run on a
backtracking engine, with a budget of 100,000 steps for each search. A
pattern that runs out stops matching in that block, so it can't stall a
run. Every other pattern runs in linear time.

## Repeat

`repeat = true` reports a word repeated right after itself, ignoring
case: "the the", "is is". The words must be separated only by
whitespace, or by emphasis (`*`, `_`, `~`) or block quote (`>`) markers.
"The the the" is one issue.

## Entries

Any `words` or `patterns` entry can be a table instead of a string:

```toml
{ match = "utilize", suggest = ["use", "employ"], message = "Simpler: {suggestion}." }
```

- **`match`:** the word or pattern.
- **`suggest`:** what to write instead, best first.
- **`message`:** the message for this entry's issues, in place of the
  rule's.

## Messages

`{match}` in a message stands for the match, `{suggestion}` for the
entry's suggestions (separated by commas), and `{rule}` for the rule's
name. `{{` and `}}` are literal braces.

- **Defaults:** an entry with suggestions and no message anywhere gets
  "Prefer: {suggestion}.", and `repeat` gets
  "Repeated word: “{match}”.".
- **Mistakes:** each of these is an error when the rule loads, never a
  blank in the output:
  - an unknown placeholder;
  - `{suggestion}` in a message for an entry with no `suggest`;
  - an entry with no message to use.

## Scope

| Scope | Blocks |
| --- | --- |
| `paragraph` | Paragraphs, footnotes, captions |
| `heading` | Headings and titles |
| `list` | List items |
| `table` | Table cells |
| `blockquote` | Block quotes |
| `code` | Code blocks, when checked with `--code-blocks` |

With no `scope`, a rule applies everywhere.

## Except

`except` lists matches that this rule never reports. Each is compared with
a whole match, never a part of one, using the rule's case:

```toml
words  = ["leverage", "leverage partners"]
except = ["Leverage Partners"]
```

A rule reports the longest match it finds, so in "ask Leverage Partners"
the match is "Leverage Partners", which `except` drops, while "leverage
it" is still reported. Without "leverage partners" in `words`, the match
would be "Leverage" and `except` wouldn't apply.

To accept a word for every rule, or only in your project, use `accept` in
[config](config.md) instead.

## Examples and `rules test`

```console
$ proserunner rules test                    # Every rule the config loads
$ proserunner rules test .proserunner/rules # Only the rules in a directory
```

Examples are Markdown, read with quoted text and code blocks checked, so
a `code` rule's example is a ```` ``` ```` fence and a `heading` rule's
starts with `#`. A bad example with no text where its rule applies says
so. For every rule, it checks that:

- each `bad` example gets at least one issue from the rule;
- each `good` example gets none;
- each `fix` pair `[before, after]` holds: replacing each issue in
  `before` with its first suggestion gives `after`.

A rule with no `bad` example is reported too, since nothing shows that it
matches. Problems are printed one per line, and the exit status is 1 when
there are any. The built-in rules' examples run in `cargo test`.

## `explain`

`proserunner explain RULE` takes an id, or a rule's name alone when only
one rule has it. It shows:

- the rule's level, and the config file that set it;
- its source, case, scope and message;
- each entry as it compiled: a literal or a regular expression, its word
  edges, and whether it backtracks;
- the examples;
- every `except`, `accept` and ignore that keeps the rule's issues from
  being reported, with the file it's in;
- the `[[overrides]]` that change the rule.
# Upgrading from 0.8

Proserunner 1.0 is a rewrite. Most of the command line is the same, and
your settings convert themselves. What changes: the file formats (TOML
instead of EDN), the rules (built in, and literal unless they say
otherwise), and what counts as prose (each format is read natively).

## Your settings convert on the first run

The first command you run converts your 0.8 settings and prints what it
wrote.

| 0.8 | 1.0 |
| --- | --- |
| `~/.config/proserunner/config.edn` | `~/.config/proserunner/config.toml` |
| `~/.config/proserunner/ignore.edn` | `~/.config/proserunner/ignores.toml` |
| `.proserunner/config.edn` | `.proserunner/config.toml`, and `.proserunner/ignores.toml` for its ignores |
| each check `NAME.edn` in a check directory | a rule `NAME.toml` beside it |

- **Backups:** each 0.8 file it replaces is renamed to `*.edn.bak`.
  Nothing is deleted.
- **Once only:** a file converts only when no TOML file is beside it, so
  this happens once.
- **All or nothing:** everything is converted in memory before anything
  is written. A file that doesn't convert stops the run with an error
  that names it, and changes nothing.
- **`~/.config/proserunner/default/`:** this holds the checks 0.8
  downloaded. 1.0 builds its rules in and never reads it, so you can
  delete it.

What the settings become:

- **`:checks`** becomes `rules`. `"default"` stays `"default"`, and a
  check directory becomes a rule directory.
- **`:files`:** a check that a `:files` list left out becomes
  `[rule."ID"] level = "off"`.
- **`:ignore`** becomes `accept` in `ignores.toml`, and **`:ignore-issues`**
  becomes `[[issue]]` entries. `:check` turns into a rule id and
  `:line-num` into `line`.
- **`:ignore-mode :replace`** and **`:config-mode :project-only`** become
  `inherit = false`, which leaves out the user config as well as the
  global ignores. In 0.8, both kept only the global ignores out, so check
  that the project doesn't rely on your user config.

To convert check files somewhere else, such as a shared repository:

```console
$ proserunner migrate rules old-checks/ new-rules/
```

## Rules are built in

0.8 downloaded its default checks on the first run, into
`~/.config/proserunner/default/`. 1.0 builds the same 22 rules into the
binary, pinned to the release.

| 0.8 | 1.0 |
| --- | --- |
| `proserunner checks` | `proserunner rules` |
| `proserunner checks add DIR` | List `DIR` in `rules` in a config, after `proserunner migrate rules` if it holds EDN checks |
| `proserunner checks restore` | Not needed. `proserunner rules export DIR` copies the built-in rules, to edit |
| A check's `:files` entry | `[rule."default/NAME"] level = "off"` |

Each rule has an id, `<source>/<name>`: `default/hedging` for the
built-in hedging rule, and `project/NAME` for a rule in a project's
`.proserunner/rules/`. Config, `explain` and inline control use ids.

## Words are literal

In 0.8, every specimen was quietly a regular expression: a `.` matched
any character, and a `?` made the letter before it optional. In 1.0:

- **`words`** are literal. Write a word as it reads, `{a,b}` for
  variants, and a space for any run of whitespace.
- **`patterns`** are regular expressions, used exactly as written.

`migrate` keeps your checks' meaning:

- a plain specimen becomes a word;
- a regular expression that stands for a few fixed strings
  (`thought leaders?`) becomes those words;
- anything else becomes a pattern with explicit `\b` word boundaries.

| 0.8 `:kind` | 1.0 rule |
| --- | --- |
| `existence` | `words` |
| `case` | `words`, with `case = "sensitive"` |
| `recommender` | `words` with `suggest` |
| `case-recommender` | `words` with `suggest`, and `case = "sensitive"` |
| `regex` | `patterns` (case-sensitive, as in 0.8) |
| `repetition` | `repeat = true` |

## More is found, and less by mistake

Each format is now read for what it is, so results differ from 0.8 in
these ways:

- **Wrapped phrases:** a phrase broken across lines, or by emphasis,
  matches: `due to the fact` at the end of one line and `that` at the
  start of the next.
- **Repeated words:** they're found in any case: "The the".
- **Case:** case folding covers every letter, so `über` matches `Über`.
- **Markup:** inline code, HTML, front matter, link targets, URLs and
  email addresses are never checked.
- **Org and TeX:** each is read as its own format, not as plain text. The
  TeX preamble, math and `\label`s aren't prose, for example. See
  [File formats](formats.md).

## Command line

| 0.8 | 1.0 |
| --- | --- |
| `--file PATH`, `-f PATH` | Pass `PATH` |
| `-v` | `-V` or `--version` |
| `--skip-ignore` | `--no-ignores` |
| `--no-cache`, `--cache-dir DIR` | Gone: there's no cache. A run is fast without one |
| `--ignore NAME` | Gone (it never did anything) |
| `--config FILE` | Still there, but it now layers over the user and project configs. To make it stand alone, put `inherit = false` in it |
| `-o edn` | Gone. Use `-o json` |
| `-o json` | Now an object, `{"version": 1, "issues": [...]}`, with end positions and byte offsets ([format](formats.md#json-output)) |
| `-o table` | Now a Markdown table |
| | New: `--fail-on LEVEL`, `-o sarif`, `-o github`, `explain`, `config`, `rules test`, `rules export` |

The commands `ignore add`, `remove`, `list`, `clear`, `audit` and `clean`
work as before, as do `init`, `--ignore-issues`, `--ignore-all`,
`--global` and `--project`. Exit statuses are the same: 0, 1 and 2.

- **`ignore clear`:** outside a terminal, it needs `--force`. 0.8 only
  warned.
- **`--ignore-issues`:** in 0.8, an issue ignored by number was ignored
  on its line. Now it's ignored in that file for that rule, wherever the
  same words appear, so edits that move it don't bring it back. Ignores
  converted from 0.8 keep their lines.
- **`init`:** it now writes `.proserunner/config.toml` and
  `.proserunner/rules/`, not `config.edn` and `checks/`.

## Environment

| 0.8 | 1.0 |
| --- | --- |
| `XDG_CONFIG_HOME` | Same |
| `PROSERUNNER_CACHE_DIR`, `XDG_CACHE_HOME`, `TMPDIR` | Gone: there's no cache |
| `HTTPS_PROXY`, `ALL_PROXY`, `NO_PROXY` | Gone: proserunner never uses the network |
| `PROSERUNNER_DEBUG` | Gone: errors say what's wrong without it |
