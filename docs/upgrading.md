# Upgrading from 0.8

Proserunner 1.0 is a rewrite. Most of the command line is the same, and
your settings convert themselves. Settings and rules are now TOML, rules
are built in, and each format is read with its own parser.

## Your settings convert on the first run

The first command you run converts your 0.8 settings and prints what it
wrote.

| 0.8 | 1.0 |
| --- | --- |
| `~/.config/proserunner/config.edn` | `~/.config/proserunner/config.toml` |
| `~/.config/proserunner/ignore.edn` | `~/.config/proserunner/ignores.toml` |
| `.proserunner/config.edn` | `.proserunner/config.toml`, and `.proserunner/ignores.toml` for its ignores |
| each check `NAME.edn` in a check directory | a rule `NAME.toml` beside it |

Each 0.8 file is renamed to `*.edn.bak`, and a file converts only when
no TOML file is beside it. Everything converts in memory first, so a
file that doesn't convert stops the run and changes nothing. 1.0 never
reads `~/.config/proserunner/default/`, so you can delete it.

- **`:checks`** becomes `rules`, and a check directory a rule directory.
- **`:files`:** a check it left out becomes
  `[rule."ID"] level = "off"`.
- **`:ignore`** becomes `accept` in `ignores.toml`, and
  **`:ignore-issues`** becomes `[[issue]]` entries.
- **`:ignore-mode :replace`** and **`:config-mode :project-only`** become
  `inherit = false`, which leaves out the user config as well as the
  global ignores, so check that the project doesn't rely on your user
  config.

To convert check files somewhere else, such as a shared repository:

```console
$ proserunner migrate rules old-checks/ new-rules/
```

## Rules

| 0.8 | 1.0 |
| --- | --- |
| `proserunner checks` | `proserunner rules` |
| `proserunner checks add DIR` | List `DIR` in `rules` in a config, after `proserunner migrate rules` if it holds EDN checks |
| `proserunner checks restore` | Not needed. `proserunner rules export DIR` copies the built-in rules to edit |
| the built-in `regex` check | `default/heading-periods` |

Each rule has an id, `<source>/<name>`, such as `default/hedging`, and
config, `explain` and inline control use it.

In 0.8, every specimen was a regular expression, so a `.` matched any
character. In 1.0, `words` are literal, and `patterns` are regular
expressions. `migrate` keeps your checks' meaning: a regular expression
for a few fixed strings (`thought leaders?`) becomes those words, and
anything else a pattern with explicit `\b` word boundaries.

| 0.8 `:kind` | 1.0 rule |
| --- | --- |
| `existence` | `words` |
| `case` | `words`, with `case = "sensitive"` |
| `recommender` | `words` with `suggest` |
| `case-recommender` | `words` with `suggest`, and `case = "sensitive"` |
| `regex` | `patterns` (case-sensitive, as in 0.8) |
| `repetition` | `repeat = true` |

What the rules find changes too: phrases match across line breaks and
markup, and code, link targets and Org drawers aren't checked. See the
[changelog](../CHANGELOG.md).

## Command line

| 0.8 | 1.0 |
| --- | --- |
| `--file PATH`, `-f PATH` | Pass `PATH` |
| `-v` | `-V` or `--version` |
| `--skip-ignore` | `--no-ignores` |
| `--no-cache`, `--cache-dir DIR` | Gone: there's no cache |
| `--ignore NAME` | Gone (it never did anything) |
| `--config FILE` | Now layers over the user and project configs, unless it has `inherit = false` |
| `-o edn` | Gone. Use `-o json` |
| `-o json` | Now an object, `{"version": 1, "issues": [...]}` ([format](formats.md#json-output)) |
| `-o table` | Now a Markdown table |
| `.tex` files | Not checked |

Other commands and options work as before, and exit statuses are the
same.

- **`ignore clear`** needs `--force` outside a terminal.
- **`--ignore-issues`** ignores an issue in its file for its rule on any
  line, not only on its line, and exits 1 when issues remain. Converted
  ignores keep their lines.
- **Levels:** hints (`overused-adverbs`, `hedging`, `weasel-words` and
  `skunked-terms`) don't make a run exit 1 unless you pass
  `--fail-on hint`.
- **`init`** writes `.proserunner/config.toml` and `.proserunner/rules/`.

## Environment

`XDG_CONFIG_HOME` works as before, and `XDG_CACHE_HOME` now holds only
recent runs' issue numbers. `PROSERUNNER_CACHE_DIR`, `TMPDIR`,
`PROSERUNNER_DEBUG` and the proxy variables are gone, since 1.0 has no
cache and never uses the network.
