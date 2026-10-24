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

[formats]
"*.mdx" = "markdown"              # MDX files are checked as Markdown

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
| `formats` | none | Formats for files by path pattern (see [Formats](#formats)) |
| `rule."ID".level` | the rule's own | `"off"`, `"hint"`, `"warning"` or `"error"`, for every entry in the rule, even one with a level of its own |
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
- **`[formats]` rows and `[[overrides]]`** add up too, and apply in
  order, so a project's come after the user config's.
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

## Formats

Proserunner reads `.md`, `.markdown`, `.mdx`, `.org`, `.txt`, `.html`,
`.htm`, `.xhtml`, `.adoc`, `.asciidoc` and `.typ` files as their
extension says. `[formats]` maps other files to a format, or overrides
an extension, by [path pattern](#path-patterns):

```toml
[formats]
"*.qmd" = "markdown"
"notes/*.txt" = "org"
```

- **Names:** `markdown`, `mdx`, `org`, `text`, `html`, `asciidoc` and
  `typst`. A misspelled one stops the run, with a "did you mean".
- **Order:** rows apply in order, so when several match a file, the last
  one wins. A row wins over the extension.
- **Walks:** a directory walk checks every file a row matches, so
  `"*.qmd"` adds Quarto files. A pattern for a whole directory adds every
  file in it, images too; give it an extension, such as
  `"legacy/*.md"`.

`--stdin-filename` uses these rows too; see
[standard input](formats.md#standard-input).

## Path patterns

`check.exclude`, `overrides.paths` and `[formats]` use gitignore's
rules:

| Pattern | Matches |
| --- | --- |
| `*.draft.md` | at any depth (no `/`) |
| `drafts/` | a directory named `drafts`, at any depth, and everything in it |
| `docs/legal` | `docs/legal` under the base directory, and everything in it |
| `/docs/*.md` | Markdown files directly in `docs/` under the base directory |
| `~/notes/**` | everything under `~/notes` |

The base directory is the project root, or the directory holding the
config, and a leading `../` climbs from it. A pattern with no `/` before
its end, such as `drafts/`, matches at any depth below the project root,
whichever config it's in, or below the working directory outside a
project. It never matches a directory that root is inside: for a file
outside the root, only the directories below the one they share count.
`*` doesn't cross a `/`, and `**` does. Negated patterns (`!…`) aren't
supported.

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
$ proserunner document.md --ignore-issues 1,3-5   # These issues, by the last run's numbers
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
- **Which numbers:** `--ignore-issues` takes the numbers the last run of
  the same paths, from the same directory, showed. So fixing an issue
  before ignoring others doesn't make the numbers name different issues.
  It lists each issue it ignores, skips one that's gone since, and then
  shows what's left, renumbered, with the exit status of a check. After a
  run of other paths, the numbers are the current run's. The last run's
  numbers are kept in `~/.cache/proserunner/` (or
  `$XDG_CACHE_HOME/proserunner/`); a run that can't write there still
  succeeds.
- **Matching:** words are compared like `accept`: whole matches, any
  case.
- **Comments:** proserunner keeps the comments you add to an
  `ignores.toml`. A file that doesn't parse is never overwritten: the
  command stops with an error instead.
- **`clear` in scripts:** outside a terminal, `ignore clear` needs
  `--force`.
- **Stale ignores:** `audit` and `clean` leave alone a relative `file`
  in the global list, such as one migrated from 0.8. It's relative to
  wherever you run proserunner, so it may name a file that's there from
  another directory.

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
