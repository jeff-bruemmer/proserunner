# Config

Proserunner needs no config. When you want one, it's TOML, in up to
three layers:

| Layer | File |
| --- | --- |
| User | `~/.config/proserunner/config.toml` (or `$XDG_CONFIG_HOME/proserunner/`) |
| Project | `.proserunner/config.toml` in the project root |
| Explicit | the file given with `--config FILE` |

The project is the nearest directory with a `.proserunner/`, from the
files you check up, stopping at your home directory and at a git root.
With only standard input, and for commands that check nothing, the
search starts at the working directory. A file in no project gets the
working directory's project, if there is one. A run has one config, so
files that get different projects, or a project and none, can't be
checked together.
`proserunner init` starts a project in the working directory.

`proserunner config` prints the merged config, with the file each value
came from.

```toml
schema  = 1
extends = ["../house-style/config.toml"]
rules   = ["default", { path = ".proserunner/rules", prefix = "project" }]
fail-on = "warning"
accept  = ["Kubernetes"]          # never reported by any rule
mask    = ['\{\{<.*?>\}\}']       # Hugo shortcodes: no rule sees them

[check]
exclude      = ["drafts/", "*.draft.md"]
quoted-text  = false
code-blocks  = false
front-matter = ["title", "description", "params.summary"]
output       = "group"

[formats]
"*.qmd" = "markdown"

[rule."default/hedging"]
level = "off"

[rule."default/jargon"]
accept = ["synergy"]              # never reported by this rule

[[overrides]]
paths = ["docs/legal/", "*.contract.md"]
accept = ["hereinafter"]
rule."default/hedging".level = "error"
```

## Settings

| Key | Default | Meaning |
| --- | --- | --- |
| `schema` | (required) | The format version: `1` |
| `inherit` | `true` | `false` leaves out the layers below this one, and their ignore lists |
| `extends` | none | Configs this one builds on (see [Extends](#extends)) |
| `rules` | `["default"]` | Where rules come from (see [Rules](rules.md#where-rules-come-from)) |
| `fail-on` | `"warning"` | The least serious level that makes a run exit 1: `"hint"`, `"warning"` or `"error"` |
| `accept` | none | Matches no rule reports |
| `mask` | none | Regular expressions for text no rule should see |
| `check.exclude` | none | Files to skip, as [path patterns](#path-patterns) |
| `check.quoted-text` | `false` | Check quoted text too |
| `check.code-blocks` | `false` | Check code blocks too |
| `check.front-matter` | `title`, `subtitle`, `description`, `summary`, `abstract`, `excerpt` | Front matter keys whose strings are prose (see [Front matter](formats.md#front-matter)) |
| `check.output` | `"group"` | The output format |
| `formats` | none | Formats for files by path pattern (see [Formats](#formats)) |
| `rule."ID".level` | the rule's own | `"off"`, `"hint"`, `"warning"` or `"error"`, for every entry in the rule |
| `rule."ID".accept` | none | Matches this rule doesn't report |
| `rule."SOURCE/*"` | none | `level` and `accept` for every rule from a source |
| `overrides` | none | Settings for some files |

Command-line options win over `check.*`, and `--exclude` adds to
`check.exclude`.

- **`accept`:** an entry matches a whole match, in any case, never part
  of one. `accept = ["synergy"]` hides "Synergy" but not "synergies".
- **`mask`:** patterns run on each block's prose, after markup is gone,
  and what they match becomes a placeholder, like inline code.
- **Rule ids:** `[rule."…"]` takes whole ids, and one that names no rule
  stops the run with a "did you mean". An id from a source this run
  doesn't load, such as a project's rules in your user config, is left
  alone, unless it's a typo away from a loaded rule.
- **`source/*`:** `[rule."default/*"]` sets every built-in rule. A
  rule's own table wins over it in any layer, so `default/*` off and
  `default/jargon` at `"warning"` keep only jargon.
- **Overrides** set `accept` and `rule."…"` for the files their `paths`
  match. When several match a file, they apply in order, and the last
  one wins.

## Changing a level from the command line

```console
$ proserunner rules set hedging off          # In the project's config, inside a project
$ proserunner rules set 'default/*' hint     # Every built-in rule
$ proserunner rules set --global jargon error
```

`rules set` writes the whole id into the project's config inside a
project, and into the user config outside one, unless you pass
`--global` or `--project`. It keeps the file's comments, and says when
another setting wins over the one it wrote.

## How layers merge

Layers apply in order: built-in defaults, user, project, then `--config`.

- **Tables** merge key by key.
- **Values and lists** replace earlier ones, except `accept`, `mask`,
  `check.exclude`, `[formats]` and `[[overrides]]`, which add up.
- **`inherit = false`** in a project config leaves out the user config
  and the global ignore list. In a `--config` file, it leaves out both
  configs and every ignore list. What the file extends still applies.

Paths in a project config are relative to the project root, and in any
other config to its directory. `~/` is your home directory.

## Extends

A config can build on others, such as a style a team shares:

```toml
# ../house-style/config.toml
schema = 1
rules = ["default", { path = "rules", prefix = "house" }]
accept = ["Kubernetes"]
```

- **Order:** each config in `extends` is a layer just below the one
  that names it, and a later one wins over an earlier one.
- **Rules** add up: a config gets the rules of those it extends, so a
  project that extends this one gets the built-in rules, `house/…` and
  its own. Turn some off with `[rule."house/*"]`.
- **Paths** in an extended config are relative to its own directory.
- **Sharing:** proserunner never uses the network, so share a style as
  files, such as a git submodule.
- **Once:** a config extended twice is read once, below everything that
  extends it.
- **Mistakes:** a missing file, a cycle, and `inherit` in an extended
  config are errors.

## Formats

`[formats]` maps a path pattern to a [format name](formats.md), for
files whose extension says nothing or the wrong thing. When several rows
match a file, the last wins, and a row wins over the extension. A
directory walk checks every file a row matches, so give a directory's
pattern an extension (`"legacy/*.md"`), or it takes images too.

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

The base directory is the project root, or the config's directory, and
`../` climbs from it. A pattern with no `/` before its end matches at
any depth below the project root, or below the working directory
outside a project. `*` doesn't cross a `/`, and `**` does. Negated
patterns (`!…`) aren't supported.

`.proserunnerignore` files (gitignore syntax) and `--exclude PATTERN`
skip files too. A file you name is checked anyway, unless you pass
`--force-exclude`, as hooks that name every changed file need.

## Ignores

Ignores are what proserunner writes for you: one global list, in
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
$ proserunner ignore clear     # Remove every ignore (asks first; --force in scripts)
$ proserunner document.md --no-ignores            # A run without ignores
```

- **Which list:** the project's inside a project, and the global one
  outside, unless you pass `--global` or `--project`.
- **By number:** an issue is ignored in its file, for its rule, wherever
  the same words appear, so edits that move it don't bring it back.
- **Which numbers:** the ones the last run of the same paths, from the
  same directory, showed. The 20 most recent runs are kept in
  `~/.cache/proserunner/runs/` (or `$XDG_CACHE_HOME/proserunner/runs/`). With none for
  these paths, the numbers are this run's.
- **Words** match like `accept`.
- **Comments** you add are kept, and a file that doesn't parse is never
  overwritten.
- **0.8 paths:** `audit` and `clean` leave alone a relative `file` in
  the global list, since it's relative to wherever you run proserunner.

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

`proserunner schema config` (or `rule`, or `ignores`) prints the JSON
Schema for the version you have, and [`schemas/`](../schemas) holds
them too. Editors built on taplo, such as VS Code with Even Better TOML,
use one to complete keys and mark mistakes. Write it next to the file,
and name it on the first line, or in a `.taplo.toml`:

```console
$ proserunner schema config > .proserunner/config.schema.json
```

```toml
#:schema ./config.schema.json
schema = 1
```
