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
them as a library. `scan` doesn't depend on `rules`: a block's `Scope`
belongs to the scanner. Errors are one `Error` enum, and only `main`
turns them into messages and exit statuses.

**Scanners** adapt an established parser for each format, and none reads
a known format by hand. Where a parser keeps something to itself, such
as a title's inline markup, the scanner parses that text again. Where a
parser decodes text, `decoded::push` lines it up with the source. Each
scanner builds its blocks with `ViewBuilder`, which collapses
whitespace, turns non-prose into placeholders, and maps each byte back
to the source, and returns `Scanned { blocks, directives }`.

**Matching:**

- `words` and `terms` from every rule go into one Aho-Corasick automaton
  per case mode.
- `patterns` go into one `RegexSet`, and a pattern runs only in blocks
  where the set matches. Patterns too large to share a set run on every
  block, with a warning.
- Lookaround runs on `fancy-regex`, with a backtracking budget; a
  pattern that runs out is recorded in `Linted::exhausted`.
- Word edges are checked after matching, so the automata stay on their
  fast paths.
- **Checks** (`src/rules/checks/`) are variants of the sealed `Check`
  enum. Their issues resolve like an entry's: per rule, the longest of
  overlapping matches wins, and `except` applies.
- Placeholders resolve to slots when a rule loads, so a message never
  renders with a blank.

## Library API conventions

The library is a supported API, so it follows [Google's Rust style
guide](https://google.github.io/styleguide/rustguide.html), including its
rules for libraries:

- **Rule indexes** are `RuleIndex`, a `u32` only the crate makes. It
  isn't `RuleId`, because a rule's `id` is its name.
- **Constructors:** one that can fail is `try_new`, and `new` never
  fails.
- **`#[non_exhaustive]`** is on every exported enum, and on every
  exported struct whose fields are all public. Structs that callers
  build have a `new` or `Default`. Closed sets, such as `Case`, stay
  exhaustive.
- **`#[must_use]`** is on functions whose result is their only effect,
  on error types, and on lazy iterators, but not on a function that
  returns one of those types (clippy's `double_must_use`).
- **Outside semver:** `contain`, `english`, `fsutil` and `migrate` are
  `#[doc(hidden)] pub`, for the binary and the fuzz targets. Helpers only
  the crate uses are `pub(crate)`.
- **Lint exceptions** are `#[expect(lint, reason = "…")]`, rustdoc's
  lints included. The one `#[allow]` is on `migrate`, which isn't API.
- **Names** aren't abbreviated, except `Ctx`. A lifetime on a type
  names what it borrows (`'src`); a function can use `'a`.
- **Calls into another module** name it: `typo::closest()`, not an
  imported `closest()`. Types are imported, except where the name alone
  is ambiguous (`io::Result`). A trait used in one function is imported
  there.
- **Docs:** every public item has one. A type in the JSON Schemas has no
  intra-doc links, because its docs become the schema's descriptions.
- **Errors:** a pass over a file reports every mistake, not the first,
  in `Error::Several`.
- **`Display`** on an enum with `as_str()` uses `f.pad`, so `{:<8}`
  pads.

The lints in `Cargo.toml` and `clippy.toml` hold most of this.

## Checks

```console
$ cargo test
$ cargo clippy --all-targets -- -D warnings
$ cargo fmt --check
$ RUSTDOCFLAGS='-D warnings' cargo doc --no-deps
$ cargo check --manifest-path fuzz/Cargo.toml --bins
```

`cargo test` covers neither doc links nor the fuzz targets. `unwrap`,
`expect` and `panic!` are denied outside tests. Each black-box test gets
its own home, config and cache directories.

Run the binary by hand with a scratch home too, since every command
first converts any 0.8 settings in `~/.config/proserunner/`:

```console
$ HOME=/tmp/pr XDG_CONFIG_HOME=/tmp/pr/.config XDG_CACHE_HOME=/tmp/pr/.cache target/debug/proserunner docs/
```

## Snapshots, schemas and built-in docs

- **Golden corpus:** `tests/golden.rs` snapshots what the built-in rules
  find in `resources/`. Review changes with `cargo insta review`, and
  add each to "Behaviour changes" in the [changelog](../CHANGELOG.md).
- **JSON Schemas:** `schemas/*.schema.json` are generated, in tests,
  from the types that read each file, and a test fails when they're out
  of date. Rewrite them with
  `PROSERUNNER_WRITE_SCHEMAS=1 cargo test json_schemas`.
- **Built-in docs:** `proserunner guide` prints `README.md` and `docs/`,
  built into the binary, so write them to read well in a terminal, and
  keep `docs/` and `schemas/` in `Cargo.toml`'s `include`.

## Fuzzing

`fuzz/` holds [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz)
targets: `scan` (a format byte, then a document) runs every scanner,
lint and output format and checks spans; `rules` compiles a rule file
and runs it; and `edn` reads a 0.8 check file. With a nightly toolchain:

```console
$ cargo +nightly fuzz run scan
```

On stable, build them with the flags `cargo fuzz` would pass, less the
sanitizers:

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

`seeds/scan/` holds the samples in `resources/`, each after a format
byte that indexes `Format::all()` (bits 0–1, then 4–7; bit 2 is quoted
text and bit 3 code blocks), so a new format shifts the ones after it.
A panic inside a parser that a scanner calls through `contain::catch`
isn't a crash. When a target finds one, add the input as a test beside
the code it broke, replay the files in `artifacts/` against the fix, and
delete them.

## Speed

`cargo bench` measures compiling the rules and linting 5,000 lines of
each format. The targets are 5 ms to lint 5,000 lines, and 10 ms for a
whole run on a 2 KB file (`scripts/check-speed`). AsciiDoc and MDX miss
the first by what their parsers cost. Compare two builds by running
them in turn, not against saved numbers.
`resources/benchmark-data/docs.md` is a frozen copy of the docs, so its
numbers stay comparable.

## Adding things

- **A built-in rule:** add a file to `defaults/` with `bad` and `good`
  examples.
- **A file format:** add a scanner in `src/scan/` that adapts an
  established parser with byte offsets, a variant of `Format`, and a row
  of `DOCUMENTS` in `src/scan/format.rs`. Add a sample to `resources/`,
  a fuzz seed, and the name to `--stdin-format`'s help in `src/cli.rs`.
- **A check:** add `src/rules/checks/<name>.rs` with its `Spec`,
  `compile`, `describe` and hooks (`wants`, for words the token tier
  finds; `block`; and `document`), a variant in `checks/check.rs`, and
  its key in `src/rules/rule_file.rs`. Regenerate the schemas and
  document it in [Rules](rules.md).
- **An output format:** add a module in `src/format/`, a variant of
  `Output`, and a row of `WRITERS` in `src/format/output.rs`.
