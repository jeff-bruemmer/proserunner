# Changelog

Every change to what the built-in rules find is listed under "Behaviour
changes", and appears in the golden snapshots in `tests/snapshots/`.

## 1.0.0 (unreleased)

A rewrite in Rust. [Upgrading from 0.8](docs/upgrading.md) covers each
change in detail.

### Behaviour changes

- Each format is read with an established parser for it: Markdown with
  pulldown-cmark, and Org with orgize. Code, HTML, front matter, link
  targets and Org drawers are no longer checked.
- Phrases match across line breaks and inline markup.
- `words` are literal, and only `patterns` are regular expressions. 0.8's
  checks convert to the same matches.
- Repeated words are found in any case ("The the"), and case folding
  covers every letter.
- An issue ignored by number is ignored in its file for its rule, on any
  line.
- URLs and email addresses are never checked, in any format, and a
  straight `"` only quotes at word edges, so `5" wide` hides nothing.
- `phrasal-adjectives` matches "health care coverage" (it had a typo).
- A word doesn't match inside a dotted name, so a rule for `github`
  leaves `github.com` and `docs.github.io` alone.
- A directory check now includes `.mdx`, `.html`, `.htm`, `.xhtml`,
  `.adoc`, `.asciidoc` and `.typ` files. Exclude them (`check.exclude`, or
  `.proserunnerignore`) to keep a build's output out.
- Built-in rules have levels: `repetition`, `annotations` and `non-words`
  are errors; `overused-adverbs`, `hedging`, `weasel-words` and
  `skunked-terms` are hints, which don't make a run exit 1 by default.
- No two built-in rules match the same words, so a phrase is reported
  once: "due to the fact that" is only `compression`'s, "it goes without
  saying" only `meta-discourse`'s, and so on for 20 phrases. A test keeps
  it that way.
- Every `redundancies` entry suggests what to write, and messages say
  what to do: "Redundant: “program” says it.", not "Pleonastic phrase."
- `compression` suggests one word for each time phrase ("after" for
  "subsequent to"), not "before, after, as" for all of them.
- `regex` is now `heading-periods`, a name that says what it checks, and
  its message says what to do: "Heading ends with a period: delete it."
  0.8 settings that name `regex` convert to it.

### Added

- Rules in TOML, with `words`, `patterns`, `terms`, `repeat`,
  suggestions, messages, scopes, exceptions and examples. `rules test`
  checks the examples.
- `terms` for names with one right spelling, such as "GitHub": any other
  case or listed spelling is reported, with the term as the suggestion.
- A pattern's groups in its messages and suggestions: `{1}`, `{name}`.
- An entry's own `case` and `level`, and `except` patterns that must
  match a whole match.
- Checks for what words can't find: `consistent` spellings, where the
  first a document uses wins; `limit`, for sentences and blocks that run
  long; `capitalization`, in sentence or title case; `acronyms` used
  before they're defined; and `metric`, a readability grade by
  Flesch-Kincaid, Gunning fog, Coleman-Liau or the automated
  readability index.
- `heading-levels`, to keep a rule to headings at some levels.
- Config in layers (user, project, `--config`), with `[rule."…"]`
  settings, `[[overrides]]` for paths, `accept`, `mask`, `inherit`, and
  `proserunner config` to show where each value came from.
- Inline control in every format: `proserunner-disable`,
  `proserunner-enable` and `proserunner-disable-next-line`.
- `explain`, `rules export`, `--fail-on`, and the `sarif`, `github` and
  `checkstyle` output formats. Checkstyle XML, for reviewdog and Jenkins,
  lists every file checked, issues or not.
- `--force-exclude`, for hooks that name files, and a pre-commit hook.
- `[formats]` in config, to check other files as one of the formats:
  `"*.qmd" = "markdown"`.
- MDX, HTML, AsciiDoc and Typst, each read with an established parser
  (markdown-rs, html5gum, asciidoc-parser and typst-syntax) and no
  external programs: titles and heading levels, lists, tables,
  admonitions, footnotes, inline markup and inline control. Every branch
  of an AsciiDoc conditional is checked.
- `--stdin-format`, and `--stdin-filename` to check standard input as a
  file, with its format, project, overrides and ignores, as editors need.
- `--no-quoted-text` and `--no-code-blocks`, to override config.
- Warnings, never silence, when a backtracking pattern gives up in a
  block, or when the patterns are too large to filter together.
- The project comes from the files checked, not the working directory.
- JSON Schemas for config, rule and ignore files.
- A library API: `scan`, `lint`, `policy` and `report` do no I/O, so
  editors and other tools can use them.
- 0.8 settings convert on the first run, and `migrate rules` converts
  check files.
- The default output shows each issue's level and rule id, in color in a
  terminal (`NO_COLOR` and `CLICOLOR_FORCE` apply), and `-o plain` ends
  each line with the rule's id. The summary counts the files checked and
  the issues at each level, and a long report names the rules and files
  that found the most.
- `--ignore-issues` takes the numbers the last run of the same paths
  showed, kept in the cache directory, lists each issue it ignores, and
  shows what's left.
- A directory named with nothing to check is a warning that names the
  extensions it doesn't read.
- `explain` shows the config that turns a rule off. `--help` groups the
  check options, and `init` says what to run next.
- `rules set RULE LEVEL`, to change a rule's level in the project or user
  config without editing it, keeping its comments.
- `extends`, for a config that builds on others, such as a team's shared
  style; it gets their rules too.
- `[rule."source/*"]`, to set every rule from a source; a rule's own
  table wins over it.
- `--rule`, to report only some rules' issues, and `--show-suppressed`, to
  see what config, ignore lists and inline control hide, and why.
- `proserunner guide` and `proserunner schema`: the docs and the JSON
  Schemas, built in, for the version installed.
- `rules export DIR RULE…`, to copy only some built-in rules, with what
  to do next, and a warning when two rules with the same text are both
  on.
- Every mistake in a config or rule file is reported in one run, and a
  rule's name where an id goes (`hedging`) gets the id it may mean.

### Removed

- TeX: `.tex` files aren't checked, since no TeX parser gives the exact
  positions an issue needs.
- Downloaded checks: the rules are built in.
- The results cache, `--no-cache`, `--cache-dir` and `-o edn`.
