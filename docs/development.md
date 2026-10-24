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
| `prose` | Words and sentences in a block's text |
| `policy` | Applies inline control, levels, `accept`, overrides and ignores to each file's issues |
| `report` | Orders and numbers the issues |
| `format` | Writes them in each output format |
| `config`, `ignores`, `ctx` | Settings, and where they live |
| `migrate` | Converts 0.8's EDN files |

`scan`, `lint`, `policy` and `report` do no I/O, so other tools can use
them as a library. `scan` doesn't depend on `rules`: a block's kind
(`Scope`) belongs to the scanner, and rules refer to it. Errors are one
`Error` enum, and only `main` turns them into messages and exit statuses.

**Scanners** adapt an established parser for each format:
pulldown-cmark (Markdown), markdown-rs (MDX), orgize (Org), html5gum
(HTML), asciidoc-parser (AsciiDoc) and typst-syntax (Typst). None reads
a known format by hand: where a parser keeps something to itself, such
as a title's inline markup, the scanner parses that text again on its
own. Each builds its blocks with `ViewBuilder`. It collapses
whitespace, drops markup, turns non-prose into placeholders, and keeps a
map from each byte of the block back to the source. A scanner returns
`Scanned { blocks, directives }`. It reads directives from its own comments
with `control::parse`. A heading block has a `level`: 0 for the document's
title, 1 for its outermost sections, and so on.

**Matching:**

- `words` and `terms` from every rule go into one Aho-Corasick automaton
  per case mode (an entry's own `case` picks its automaton; terms are
  always in the case-insensitive one).
- `patterns` go into one `RegexSet`, and a pattern runs only in blocks
  where the set says it matches. If the patterns are too large to share
  one set, each runs on every block instead, and `Compiled::warnings`
  says so.
- Patterns that need lookaround run on `fancy-regex`, with a backtracking
  budget. A pattern that runs out stops in that block and is recorded in
  `Linted::exhausted`; `check` warns about it where its rule is on.
- Word edges are checked after matching, so the automata stay on their
  fast paths. A word edge also rejects a dotted name (`github.com`).
- A pattern whose message or suggestions use its groups is marked
  `Target::captures`, and only those run `captures_iter`; every other
  pattern stays on `find_iter`.
- **Checks** (`src/rules/checks/`) find what words and patterns can't,
  such as a repeated word. Each is a variant of the sealed `Check` enum
  and runs on every block its rule applies to. Its issues then resolve
  like an entry's: per rule, the longest of overlapping matches wins, and
  `except` applies. A term written right is dropped at the same step, after
  it has claimed its span.
- An issue records what found it (`EntryRef::Entry` or
  `EntryRef::Check`), its entry's level, and, in `args`, the values of a
  check's own message placeholders or a pattern's groups. Placeholders resolve to slots when a rule loads, so a
  message never renders with a blank.

## Library API conventions

The library is a supported API, so it follows [Google's Rust style
guide](https://google.github.io/styleguide/rustguide.html), including its
rules for libraries:

- **Rule indexes** are `RuleIndex`, a `u32` that only the crate makes.
  `Issue.rule`, `Rule.index` and `Policy::level` use it. It isn't
  `RuleId`, because a rule's `id` is its name, such as `default/hedging`.
- **`RuleSet::compile`** returns `Compiled { rules, errors, warnings }`.
  It's `#[must_use]`, so the rule files that failed can't go unread.
  `RuleSet` is opaque: `rules()`, `get()`, `find()` and `len()` read it.
- **Constructors:** one that can fail is `try_new` (`Policy`, `Mask`), and
  `new` never fails.
- **`#[non_exhaustive]`** is on every exported enum, and on every
  exported struct whose fields are all public, so a release can add a
  variant or a field. Structs that callers build have a `new`
  (`RuleSource`, `Document`, `ignores::Entry`, `Filters`,
  `explain::List`), and `scan::Options` starts from `Default`. Closed
  sets stay exhaustive, so callers can match them: `Case`, `EntryRef`,
  `walk::Input`, `RuleDir` and `SourceSpec`.
- **`#[must_use]`** is on functions whose result is their only effect
  (`scan`, `lint::document`, `report::build`), on `Error` and
  `ControlError`, and on the lazy iterators `Words` and `Sentences`. A
  function that returns one of those types doesn't repeat it: clippy's
  `double_must_use` flags that.
- **Outside semver:** `english`, `fsutil` and `migrate` are
  `#[doc(hidden)] pub`, for the binary and the fuzz targets. Helpers only
  the crate uses are `pub(crate)`.
- **Lint exceptions** are `#[expect(lint, reason = "…")]`, so one that no
  longer applies is reported. That includes rustdoc's lints: rustdoc
  checks those expectations, and rustc skips them. The one `#[allow]` is
  on `migrate`: the API lints don't apply to a module that isn't API,
  whatever it holds.
- **Names** aren't abbreviated, except `Ctx`, the usual Rust name for a
  context. A lifetime on a type names what it borrows (`'src`,
  `'report`); a function can use `'a` to tie an input to its output.
- **Calls into another module** name it: `typo::closest()`, not an
  imported `closest()`. Types are imported, except where the name alone
  would be ambiguous (`io::Result`, `fmt::Result`). A trait used in one
  function is imported there.
- **Docs:** every public item has one. A type in the JSON Schemas has no
  intra-doc links, because its docs become the schema's descriptions
  word for word.
- **`Display`** on an enum with `as_str()` uses `f.pad`, so `{:<8}` still
  pads.

`Cargo.toml` turns on lints that hold most of this:

- **The API:** `missing_docs`, `missing_debug_implementations`,
  `unreachable_pub`, and clippy's `exhaustive_enums` and
  `exhaustive_structs`.
- **Paths:** clippy's `absolute_paths` (over two segments, set in
  `clippy.toml`) and `wildcard_imports`.
- **Plain code:** clippy's `needless_for_each`, `format_push_string` and
  `implicit_clone`, and `unsafe_code`, which is forbidden.

## Checks

```console
$ cargo test
$ cargo clippy --all-targets -- -D warnings
$ cargo fmt --check
$ RUSTDOCFLAGS='-D warnings' cargo doc --no-deps
$ cargo check --manifest-path fuzz/Cargo.toml --bins
```

The last two check doc links and the fuzz targets, which are a crate of
their own; `cargo test` covers neither. `unwrap`, `expect` and `panic!`
are denied outside tests. The tests are
hermetic: each black-box test gets its own home and config directory.

To run the binary by hand, give it a scratch home too. Every command
first converts any 0.8 settings it finds, and with your real `HOME` that
means your own `~/.config/proserunner/`:

```console
$ HOME=/tmp/pr XDG_CONFIG_HOME=/tmp/pr/.config target/debug/proserunner docs/
```

## The golden corpus

`tests/golden.rs` snapshots what the built-in rules find in `resources/`:
a sample in each format. A change to matching or scanning shows up
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
| `scan` | A format byte, then a document | Every scanner, lint and output format runs without panicking; every span, and every sentence of a block, lies on character boundaries; only headings have a level; Checkstyle output parses as XML |
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
(`0` Markdown, `1` MDX, `2` Org, `3` text, `0x10` HTML, `0x11` AsciiDoc,
`0x12` Typst; add `4` for quoted text and `8` for code blocks). Bits 0–1
and then 4–7 index `Format::all()`, the `DOCUMENTS` table in
`src/scan/format.rs`, so a new format shifts the ones after it.
`corpus/` is what a
run grows, and isn't kept. When a target finds a crash, add the input as
a regular test beside the code it broke, replay `artifacts/` against the
fix (pass the files instead of a directory), and then delete them.

## Speed

```console
$ cargo bench
```

The targets are 5 ms to lint 5,000 lines (`lint 5k lines`), and 10 ms for
a whole run on a 2 KB file. Compiling the built-in rules is a fixed cost
of every run.

| Bench | What it measures | Time |
| --- | --- | --- |
| `compile built-in rules` | The fixed cost of every run | 2.38 ms |
| `compile 500 rules` | The built-ins plus 478 generated rules, about 10,000 words and 200 patterns | 27.3 ms |
| `compile + lint 2 KB` | A whole run on a small file, less process start and I/O | 2.49 ms |
| `lint 5k lines` | `xlarge.md`: one long paragraph of drivel | 2.66 ms |
| `lint 5k lines (docs)` | `docs.md`: these docs, with headings, lists, tables and code | 3.69 ms |
| `lint 5k lines (org)` | The Org sample, repeated | 3.10 ms |
| `lint 5k lines (html)` | The HTML sample, repeated | 2.43 ms |
| `lint 5k lines (asciidoc)` | The AsciiDoc sample, repeated | 6.78 ms |
| `lint 5k lines (mdx)` | The MDX sample, repeated | 19.7 ms |
| `lint 5k lines (typst)` | The Typst sample, repeated | 2.69 ms |
| `lint 5k lines (500 rules)` | `xlarge.md`, with the 500 rules | 3.19 ms |
| `lint 5k lines (checks)` | `docs.md`, with two more rules that turn on every check | 6.52 ms |

These were measured on an AMD Ryzen 7 9800X3D, 2026-10-23. A whole cold
run on 2 KB (`scripts/check-speed`) takes 3.6 ms. Most of a format's
time is its parser: for 5,000 lines, orgize takes 1.3 ms, html5gum 1 ms,
asciidoc-parser 5.4 ms and markdown-rs 15 ms, so AsciiDoc and MDX miss
the 5 ms target by what their parsers cost. The 500 generated rules
never match, so the 500-rule numbers are the cost of rules alone. Most of
their compile time is patterns: each `Regex` costs about 75 µs to build,
twice what it adds to the shared set.

Checks cost by what they read. Against `lint 5k lines (docs)`, one rule
with only `consistent` adds 0.03 ms, since the token tier finds its
spellings; `capitalization` in headings adds 0.06 ms, `acronyms`
0.35 ms, another `repeat` 0.46 ms, `limit` 0.8 ms, and `metric`, which
counts every word's syllables, 1.3 ms.

`resources/benchmark-data/docs.md` is a frozen copy of the README and
`docs/`, so its numbers stay comparable as the docs change.

## Adding things

- **A built-in rule:** add a file to `defaults/`. `build.rs` embeds every
  file there. Give it `bad` and `good` examples: `cargo test` runs them,
  and the golden snapshot shows what it finds.
- **A file format:** add a scanner in `src/scan/` that adapts an
  established parser for it, one that gives byte offsets for its text and
  inline markup, and returns `Scanned`, with a level for each heading. Then add a variant of
  `Format` and a row of `DOCUMENTS` in `src/scan/format.rs`, with its name
  (for `[formats]` and `--stdin-format`) and extensions. The fuzz target
  and the error for unsupported files read the table. Add a sample to
  the golden corpus, and a seed for the fuzz target. Add the name to
  `--stdin-format`'s help in `src/cli.rs`; a test checks it's there.
- **A check:** add `src/rules/checks/<name>.rs` with its table in rule
  files, its `Spec` (its key, its own placeholders, its default message
  and what it suggests), `compile`, its hooks, `describe` for `explain`,
  and its tests. A check has up to three hooks: `wants` names words or
  patterns for the token tier to find, in the same pass as every rule's
  entries; `block` finds issues in one block; and `document` finds them
  across a document, from the blocks its rule applies to and what its
  wants found there. Then add a variant and its match arms in
  `checks/check.rs`, and its key to `RuleFile` in `src/rules/rule_file.rs`.
  Regenerate the JSON Schemas and document it in [rules](rules.md).
- **An output format:** add a module in `src/format/`, a variant of
  `Output`, and a row of `WRITERS` in `src/format/output.rs`. Every format
  gets the same numbered `Report`, and `Report.files` lists every file
  checked, including those with no issues.
