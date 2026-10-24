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
The built-in `regex` check is now `default/heading-periods`, and 0.8
settings that name it convert to that.

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
- **Org:** it's read as its own format, not as plain text: drawers,
  source blocks and link targets aren't prose, for example. See
  [File formats](formats.md).
- **TeX:** `.tex` files aren't checked. 1.0 reads each format with an
  established parser, and none for TeX gives the exact positions an
  issue needs.

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
| | New: `--fail-on LEVEL`, `--rule RULE`, `--show-suppressed`, `-o sarif`, `-o github`, `-o checkstyle`, `explain`, `config`, `rules set`, `rules test`, `rules export`, `guide`, `schema` |

The commands `ignore add`, `remove`, `list`, `clear`, `audit` and `clean`
work as before, as do `init`, `--ignore-issues`, `--ignore-all`,
`--global` and `--project`. Exit statuses are the same: 0, 1 and 2.

- **`ignore clear`:** outside a terminal, it needs `--force`. 0.8 only
  warned.
- **`--ignore-issues`:** in 0.8, an issue ignored by number was ignored
  on its line. Now it's ignored in that file for that rule, wherever the
  same words appear, so edits that move it don't bring it back. Ignores
  converted from 0.8 keep their lines. Its numbers are the ones the last
  run of the same paths showed, it lists what it ignores, and it shows
  what's left, so it exits 1 when issues remain.
- **Levels:** built-in rules have levels. Hints (`overused-adverbs`,
  `hedging`, `weasel-words` and `skunked-terms`) don't make a run exit 1
  unless you pass `--fail-on hint`.
- **`init`:** it now writes `.proserunner/config.toml` and
  `.proserunner/rules/`, not `config.edn` and `checks/`.

## Environment

| 0.8 | 1.0 |
| --- | --- |
| `XDG_CONFIG_HOME` | Same |
| `XDG_CACHE_HOME` | Where the last run's issue numbers are kept, for `--ignore-issues`; nothing else is cached |
| `PROSERUNNER_CACHE_DIR`, `TMPDIR` | Gone |
| | New: `NO_COLOR` and `CLICOLOR_FORCE`, for color in a terminal |
| `HTTPS_PROXY`, `ALL_PROXY`, `NO_PROXY` | Gone: proserunner never uses the network |
| `PROSERUNNER_DEBUG` | Gone: errors say what's wrong without it |
