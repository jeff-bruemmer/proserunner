# Changelog

Every change to what the built-in rules find is listed under "Behaviour
changes", and appears in the golden snapshots in `tests/snapshots/`.

## 1.0.0 (unreleased)

A rewrite in Rust. See [Upgrading from 0.8](docs/upgrading.md).

### Behaviour changes

- Each format is read with an established parser. Code, HTML, link
  targets, URLs, email addresses and Org drawers aren't checked, and of
  front matter, only keys such as `title` and `description` are.
- Phrases match across line breaks and inline markup.
- `words` are literal, and only `patterns` are regular expressions. 0.8's
  checks convert to the same matches.
- Repeated words are found in any case ("The the"), and case folding
  covers every letter.
- A straight `"` quotes only at word edges, so `5" wide` hides nothing.
- A word doesn't match inside a dotted name such as `github.com`.
- An issue ignored by number is ignored in its file for its rule, on any
  line.
- A directory check includes `.mdx`, `.html`, `.htm`, `.xhtml`, `.adoc`,
  `.asciidoc` and `.typ` files.
- Built-in rules have levels: `repetition`, `annotations` and `non-words`
  are errors, and `overused-adverbs`, `hedging`, `weasel-words` and
  `skunked-terms` are hints, which don't make a run exit 1.
- No two built-in rules match the same words, so "due to the fact that"
  is only `compression`'s, and so on for 20 phrases.
- Messages say what to do ("Redundant: “program” says it."), every
  `redundancies` entry suggests a fix, and `compression` suggests one
  word for each time phrase.
- `regex` is now `heading-periods`, and 0.8 settings that name it
  convert.
- `phrasal-adjectives` matches "health care coverage" (it had a typo).

### Added

- Rules in TOML: `words`, `patterns`, `terms`, suggestions, messages,
  scopes, `heading-levels`, exceptions and examples, tested with
  `rules test`.
- Checks: `repeat`, `consistent`, `limit`, `capitalization`, `acronyms`
  and `metric`.
- Config in layers, with `extends`, `[rule."…"]` and `source/*`,
  `[[overrides]]`, `accept`, `mask`, `inherit` and `[formats]`, and
  `proserunner config` to show where each value came from.
- MDX, HTML, AsciiDoc and Typst, and prose in YAML and TOML front
  matter.
- Inline control in every format.
- `explain`, `rules set`, `rules export`, `guide`, `schema`, `--rule`,
  `--show-suppressed`, `--fail-on`, `--force-exclude`,
  `--stdin-format` and `--stdin-filename`.
- `sarif`, `github` and `checkstyle` output, and a pre-commit hook.
- Levels and rule ids in the output, color, and a summary that counts
  files and names the rules that found the most.
- JSON Schemas for config, rule and ignore files.
- A library API: `scan`, `lint`, `policy` and `report` do no I/O.
- 0.8 settings convert on the first run, and `migrate rules` converts
  check files.

### Removed

- TeX: `.tex` files aren't checked, since no TeX parser gives the exact
  positions an issue needs.
- Downloaded checks: the rules are built in.
- The results cache, `--no-cache`, `--cache-dir` and `-o edn`.
