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

The numbers are the last run's, so fixing an issue first doesn't change
what they name.

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
$ proserunner --stdin-filename docs/a.md < draft   # Check standard input as docs/a.md
$ proserunner docs/ --rule hedging      # Only one rule's issues
$ proserunner docs/ --show-suppressed   # Also what config and ignores hide, and why
$ proserunner explain hedging           # What a rule matches, and what changes it
$ proserunner rules                     # Every rule and its level
$ proserunner config                    # The merged config, and where each value came from
$ proserunner init                      # Start a project config in .proserunner/
```

Directories skip hidden files, files over 10 MB, and files listed in
`.proserunnerignore`, `.ignore`, or git's ignore files. A file you name
is checked anyway, unless you pass `--force-exclude`. Results go to
standard output, and the summary and errors to standard error.
`NO_COLOR` turns color off, and `CLICOLOR_FORCE` keeps it in a pipe.

| Exit status | Meaning |
| --- | --- |
| `0` | No issues at `--fail-on` (default `warning`) or above |
| `1` | Issues at `--fail-on` or above |
| `2` | An error, such as bad flags, a missing file or a broken config |

Phrases match across line breaks and inline markup. Markup and link
targets are never checked, and code and quoted text only with
`--code-blocks` and `--quoted-text`. See [File formats](docs/formats.md).

## Rules

22 rules are built in, pinned to the release: `annotations`,
`archaisms`, `cliches`, `compression`, `corporate-speak`,
`heading-periods`, `hedging`, `jargon`, `meta-discourse`,
`needless-variants`, `nominalization`, `non-words`, `not-the-negative`,
`overused-adverbs`, `oxymorons`, `phrasal-adjectives`, `pompous-diction`,
`redundancies`, `repetition`, `sexism`, `skunked-terms` and
`weasel-words`. Each one's id is `default/<name>`, and
`proserunner explain NAME` shows what it matches.

`repetition`, `annotations` and `non-words` are errors.
`overused-adverbs`, `hedging`, `weasel-words` and `skunked-terms` are
hints, which don't make a run exit 1. The rest are warnings.

## Make it yours

- **Config:** turn rules off, change their levels, and accept words
  ([Config](docs/config.md)). `proserunner rules set hedging off` writes
  it for you. A config can `extends` a team's shared one.

  ```toml
  # .proserunner/config.toml
  schema = 1
  extends = ["../house-style/config.toml"]
  accept = ["Kubernetes"]

  [rule."default/hedging"]
  level = "off"

  [[overrides]]
  paths = ["docs/legal/"]
  rule."default/jargon".level = "error"
  ```

- **Your own rules:** words, patterns and terms, and checks for mixed
  spellings, long sentences, heading case, undefined acronyms and
  readability ([Rules](docs/rules.md)).

  ```toml
  # .proserunner/rules/house-style.toml  →  id "project/house-style"
  schema = 1
  message = "House style: “{suggestion}”, not “{match}”."
  words = [{ match = "e-mail", suggest = ["email"] }]

  [examples]
  bad = ["Send an e-mail."]
  good = ["Send an email."]
  ```

- **In a document:** a comment turns rules off
  ([Inline control](docs/formats.md#inline-control)).

  ```markdown
  <!-- proserunner-disable-next-line default/cliches -- quoting the brief -->
  ```

- **Ignores:** by number (`--ignore-issues 1,3`), all at once
  (`--ignore-all`), or a word everywhere (`proserunner ignore add
  hopefully`). See [Ignores](docs/config.md#ignores).

Mistakes are never silent: a typo in config, a rule file or a directive
is an error that names the file and line and suggests the closest match,
and the run exits 2.

## Editors and CI

| `-o` | For |
| --- | --- |
| `group` | People (the default) |
| `plain` | `path:line:col: [n] "match" -> message (rule)`, for Vim's quickfix, Emacs compile mode and grep |
| `table`, `verbose` | Markdown reports |
| `json` | Programs ([format](docs/formats.md#json-output)) |
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

`proserunner schema config` (or `rule`, or `ignores`) prints a JSON
Schema that editors use to check and complete the files
([Editor support](docs/config.md#editor-support)).

## Docs

`proserunner guide TOPIC` prints each of these for the version you have.

- [Config](docs/config.md): layers, settings, extends, overrides, ignores
- [Rules](docs/rules.md): writing and testing rules
- [File formats](docs/formats.md): what counts as prose, inline control, JSON and Checkstyle output
- [Upgrading from 0.8](docs/upgrading.md)
- [Development](docs/development.md)
- [Changelog](CHANGELOG.md)
