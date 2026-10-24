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
starts a project in the working directory. Any layer can build on other
configs, such as a team's shared style, with
[`extends`](#extends).

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
| `extends` | none | Configs this one builds on (see [Extends](#extends)) |
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
| `rule."SOURCE/*"` | none | `level` and `accept` for every rule from a source, such as `default/*` |
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
  one stops the run, with a "did you mean" when a rule's id is close. So
  does a name without its source, such as `hedging`, and the error names
  the ids it may mean: config takes whole ids, so adding a rule source
  never changes what one names. An id from a rule source this run
  doesn't load, such as a project's rules in your user config, is left
  alone, so one config can serve several projects, unless it's a typo
  away from a loaded rule (`defualt/hedging`), which stops the run too.
- **Every rule from a source:** `[rule."default/*"]` sets a `level` or
  `accept` for every built-in rule, and `[rule."project/*"]` for every
  rule in a project's directory. A rule's own `[rule."…"]` table wins
  over it, in any layer, so `level = "off"` for `default/*` and
  `level = "warning"` for `default/jargon` keep only jargon. `*` stands
  only for a whole name: `default/h*` is an error. Overrides take these
  too.
- **Mistakes:** a config with several mistakes reports each of them in
  one run, with the file each is in. A TOML mistake (bad syntax, an
  unknown key, a value of the wrong type) stops at the first in its
  file, since nothing after it can be read.

## Changing a level from the command line

```console
$ proserunner rules set hedging off          # In the project's config, inside a project
$ proserunner rules set 'default/*' hint     # Every built-in rule
$ proserunner rules set --global jargon error
```

`rules set` writes `[rule."ID"] level` into the project's config inside a
project, and into the user config outside one; `--global` and
`--project` choose. It takes a rule's id, its name when only one rule
has it, or `source/*`, and it writes the whole id. It keeps the file's
comments and layout, and makes the file if there's none. When another
layer's setting wins over the one it wrote, it says so.

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
- **`extends`:** each config a layer extends is a layer just below it,
  and `rules` add up within them (see [Extends](#extends)).

Paths in a project config are relative to the project root. Paths in any
other config are relative to the directory that holds it. `~/` is your
home directory.

## Extends

A config can build on others, such as a style a team shares across its
repositories:

```toml
# .proserunner/config.toml
schema = 1
extends = ["../house-style/config.toml"]
rules = ["default", { path = ".proserunner/rules", prefix = "project" }]
```

```toml
# ../house-style/config.toml
schema = 1
rules = ["default", { path = "rules", prefix = "house" }]
accept = ["Kubernetes"]

[rule."default/hedging"]
level = "off"
```

- **Order:** each config in `extends` is a layer just below the config
  that names it, in order. So the config that names them wins over them,
  and a later one wins over an earlier one. A config can extend others
  in turn.
- **Rules:** `rules` add up: a config gets the rules of the configs it
  extends, then its own, so the project above checks with the built-in
  rules, `house/…` and `project/…`. To leave some out, turn them off,
  such as with `[rule."house/*"]`. Between the user config, the project
  config and `--config`, a later `rules` still replaces an earlier one.
- **Paths:** `extends` paths are relative to the project root in a project
  config, and to the config's directory in any other. Paths in an
  extended config are relative to its own directory, and its path
  patterns without a `/` match below the project root.
- **Sharing:** proserunner never uses the network, so share a style as
  files: a git submodule, or a checkout next to your projects.
- **Once:** a config extended twice, as when a project and its team's
  style both extend one base, is read once, below everything that
  extends it.
- **Mistakes:** a missing file, a config that extends itself through
  another, and `inherit` in an extended config are errors. `inherit`
  decides which layers apply, so it goes in a config named on its own.

`proserunner config` lists every file it merged, extended ones too.

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

JSON Schemas describe each file. `proserunner schema` prints the one for
the version you have, and [`schemas/`](../schemas) holds them too:

| File | Schema |
| --- | --- |
| `config.toml` | `proserunner schema config`, `schemas/config.schema.json` |
| rule files | `proserunner schema rule`, `schemas/rule.schema.json` |
| `ignores.toml` | `proserunner schema ignores`, `schemas/ignores.schema.json` |

Editors that read TOML schemas, such as VS Code with Even Better TOML,
and anything else built on taplo, then complete keys, show what each one
means, and mark mistakes as you type. Write the schema next to the file
and name it on the first line:

```console
$ proserunner schema config > .proserunner/config.schema.json
```

```toml
#:schema ./config.schema.json
schema = 1
```

Or write each schema into the project's `schemas/` and tell taplo
once, in a `.taplo.toml` at the project root:

```toml
[[rule]]
include = ["**/.proserunner/config.toml"]
schema.path = "./schemas/config.schema.json"

[[rule]]
include = ["**/.proserunner/rules/*.toml"]
schema.path = "./schemas/rule.schema.json"
```
