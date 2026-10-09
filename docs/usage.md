# Usage

Everything you need to use Proserunner.

## Quick reference

```bash
# Check files - issues get numbers
proserunner document.md
proserunner docs/ README.md        # Several paths, or a shell glob like *.md
# Output:
# document.md
# [1]  10:5   "utilize"  -> Consider using "use" instead.
# [2]  15:12  "leverage" -> Consider using "use" instead.
# Checked 1 file, found 2 issues.   (on stderr)

# Ignore by number
proserunner document.md --ignore-issues 1,3
proserunner document.md --ignore-issues 1-3,5  # Ranges work too

# Ignore everything currently showing
proserunner document.md --ignore-all
proserunner document.md --ignore-all --global  # Force global scope

# One issue per line, for grep or your editor
proserunner docs/ -o plain

# Ignore a word everywhere
proserunner ignore add hopefully

# Clean up old ignores
proserunner ignore audit
proserunner ignore clean

# Check text from another program
pandoc -t markdown report.docx | proserunner -

# Check quoted dialogue too
proserunner document.md --quoted-text

# Skip files/directories
proserunner docs/ --exclude "drafts/*"
proserunner docs/ --exclude "drafts/*,*.backup,temp.md"  # Comma-separated
```

`--file PATH` still works and means the same as passing `PATH`.

## Commands

| Command | What it does |
| --- | --- |
| `proserunner [check] PATH...` | Check files and directories; `-` reads standard input. `check` is optional |
| `proserunner ignore add SPECIMEN` | Ignore a word or phrase everywhere |
| `proserunner ignore remove SPECIMEN` | Stop ignoring it (alias: `rm`) |
| `proserunner ignore list` | List ignores (alias: `ls`) |
| `proserunner ignore clear` | Remove every ignore |
| `proserunner ignore audit` | List ignores for files that no longer exist |
| `proserunner ignore clean` | Remove them |
| `proserunner checks` | List enabled checks (also `checks list`) |
| `proserunner checks add DIR` | Import the `.edn` checks in `DIR` |
| `proserunner checks restore` | Reinstall the default checks this version ships with |
| `proserunner init` | Set up `.proserunner/` for a project |
| `proserunner help [COMMAND]` | Show help, or one command's help |

Options can go anywhere on the line. To check a file or directory that has a command's name, such as `checks/`, write `./checks`. Upgrading from 0.7 or earlier? See [Upgrading](#upgrading-from-07).

## Standard input

Pass `-` as the path to check text that another program writes:

```bash
pandoc -t markdown report.docx | proserunner -
pbpaste | proserunner - -o plain
```

Results are labeled `<stdin>`. The text is checked like a markdown file, and it isn't cached. `--ignore-issues` and `--ignore-all` don't work with it, because those ignores belong to a file.

## Output and exit status

Lint results go to **stdout**. Status messages, warnings, errors, the summary line, and `--timer` go to **stderr**, so piping the results never picks up anything else.

| `--output` | What you get |
| --- | --- |
| `group` (default) | Issues grouped under each file name, numbered |
| `plain` | One issue per line: `path:line:col: [n] "specimen" -> message`. Works with grep, vim's quickfix, and Emacs compile-mode |
| `table` | A bordered table |
| `verbose` | Markdown report with fixes |
| `json`, `edn` | Machine-readable; `[]` / `()` when there are no issues |

| Exit status | Meaning |
| --- | --- |
| `0` | No issues found |
| `1` | Issues found |
| `2` | Error: bad flags, missing or unsupported files, broken config |

So in CI, `proserunner docs/` fails the build when there are issues. Add `--quiet` to drop the status messages and summary line (errors still print).

Running `proserunner` with no arguments prints a short usage message; `proserunner --help` prints everything. `-h` works at the end of any command line, and after a command it shows that command's help.

## Default checks

Ships with 18 checks. See what's enabled: `proserunner checks`

Full list: [github.com/jeff-bruemmer/proserunner-default-checks](https://github.com/jeff-bruemmer/proserunner-default-checks)

| Name               | Kind        | Explanation                                                                                     |
| ------------------ | ----------- | ----------------------------------------------------------------------------------------------- |
| Annotations        | Case        | Typical markers used to signal problem sites in the text.                                       |
| Archaisms          | Existence   | Outmoded word or phrase.                                                                        |
| Clichés            | Existence   | An overused phrase or opinion that betrays a lack of original thought.                          |
| Compression        | Recommender | Compressing common phrases. From Style: Toward Clarity and Grace by Joseph M. Williams.         |
| Corporate-speak    | Existence   | Words and phrases that make you sound like an automaton.                                        |
| Hedging            | Existence   | Say, or say not. There is no hedging.                                                           |
| Jargon             | Existence   | Phrases infected with bureaucracy.                                                              |
| Needless-variant   | Recommender | Prefer the more common term.                                                                    |
| Non-words          | Recommender | Identifies sequences of letters masquerading as words, and suggests an actual word.             |
| Not the negative.  | Recommender | Prefer the word to the negation of the word's opposite.                                         |
| Overused-adverbs   | Existence   | Use of adverbs that are weak or redundant. Consider using a stronger verb instead.              |
| Oxymorons          | Existence   | Avoid contradictory terms (that aren't funny).                                                  |
| Phrasal adjectives | Recommender | Hyphenate phrasal adjectives.                                                                   |
| Pompous-diction    | Recommender | Pompous diction: use simpler words. From Style: Toward Clarity and Grace by Joseph M. Williams. |
| Redundancies       | Existence   | Avoid phrases that say the same thing more than once.                                           |
| Repetition         | Repetition  | Catches consecutive repetition of words, like _the the_.                                        |
| Sexism             | Existence   | Sexist or ridiculous terms (like _mail person_ instead of _mail carrier_).                      |
| Skunked-terms      | Existence   | Words with controversial correct usage that are best avoided.                                   |

## Check types

Checks are EDN files. Different types do different things:

| Type             | What it does                              |
| :--------------- | :---------------------------------------- |
| existence        | Flag specific words/phrases               |
| case             | Same but case-sensitive                   |
| recommender      | Suggest replacements (avoid X → prefer Y) |
| case-recommender | Recommender but case-sensitive            |
| repetition       | Catch "the the" style duplication         |
| regex            | Custom regex patterns                     |

## Quoted text

By default, quoted text gets skipped. Checks your narrative, ignores dialogue.

Example:

```
She said "obviously this is wrong" and walked away.
```

Gets checked as:

```
She said                         and walked away.
```

Quoted parts become spaces (preserves column numbers). Line stays intact.

Want to check quotes too? Use `--quoted-text`:

```bash
proserunner document.md --quoted-text
```

Works with straight quotes (`"..."`, `'...'`) and curly quotes (`"..."`, `'...'`).

## Config

The global config lives in `$XDG_CONFIG_HOME/proserunner/`, which is `~/.config/proserunner/` unless you've set `XDG_CONFIG_HOME`. The first run downloads the default checks there:

```
~/.config/proserunner/
├── config.edn      # Main config
├── ignore.edn      # Global ignores
├── custom/         # Your checks
├── default/        # Default checks
└── backups/        # Old default checks, from checks restore
```

Earlier releases used `~/.proserunner/`. The first run of this release moves it to the new place. If it can't (for example, because the two are on different filesystems), Proserunner keeps using `~/.proserunner/` and prints the `mv` command to move it yourself.

Two runs can safely change ignores or config at once, say from an editor and a terminal: each update holds a lock (`.lock` in the config directory), so neither overwrites the other's changes.

### Restore default checks

```bash
proserunner checks restore
```

Reinstalls the default checks and replaces `default/`, after copying the old one to `backups/`. Your `config.edn`, `ignore.edn`, and `custom/` checks are kept. If the download fails or is interrupted, your current checks stay in place; run it again.

Each release of Proserunner is pinned to one version of the [default checks](https://github.com/jeff-bruemmer/proserunner-default-checks), and it verifies a checksum of what it downloads. So results never change under you: newer checks come with newer releases.

Behind a proxy? Downloads go through `HTTPS_PROXY` (or `ALL_PROXY`) and skip hosts listed in `NO_PROXY`, the same way curl does. Proxies that need a username and password aren't supported.

### Turn off checks

Edit `~/.config/proserunner/config.edn`:

```clojure
{:checks
 [{:name "default"
   :directory "default"
   :files ["cliches"
           ;; "jargon"      ; disabled
           "corporate-speak"]}]}
```

## Ignoring stuff

Two kinds: **simple** (ignore everywhere) and **contextual** (ignore at specific spots).

### Simple ignores

```bash
proserunner ignore add hopefully
proserunner ignore add "very unique"   # Quote phrases
proserunner ignore remove hopefully
proserunner ignore list
```

### Contextual ignores

Ignore by issue number:

```bash
proserunner document.md
# [1]  10:5   "utilize"  -> Consider using "use" instead.
# [2]  15:12  "leverage" -> Consider using "use" instead.

proserunner document.md --ignore-issues 1,2    # Ignore 1 and 2
proserunner document.md --ignore-issues 1-5,8  # Ranges work
proserunner document.md --ignore-all           # Ignore everything shown
```

Issue numbers are only valid for the current run. The system stores the actual location (file:line:col:specimen), so the next run will renumber remaining issues.

**Scope:**

- Default: project if `.proserunner/` exists, else global
- Force with `--global` or `--project`

### Clean up ignores

```bash
proserunner ignore audit  # Find ignores for files that no longer exist (changes nothing)
proserunner ignore clean  # Remove them
```

Like the other `ignore` commands, these use the project list inside a project, otherwise the global list.

### Clear all ignores

```bash
proserunner ignore clear          # Asks first when run in a terminal
proserunner ignore clear --force  # Don't ask
```

Clears the project list inside a project, otherwise the global list. Add `--global` or `--project` to choose.

In scripts, pass `--force`. Without it, `ignore clear` still clears when it isn't run in a terminal, but prints a warning: a future release will require `--force` there.

### Edit manually

Global (`~/.config/proserunner/ignore.edn`):

```clojure
{:ignore #{"hopefully"}                                          ; Simple
 :ignore-issues [{:file "docs/api.md" :line-num 42 :specimen "utilize"}]}  ; Contextual
```

Project (`.proserunner/config.edn`):

```clojure
{:ignore #{"project-term"
           {:file "docs/internal.md" :line-num 5 :specimen "utilize"}}
 :ignore-mode :extend}  ; :extend (merge with global) or :replace
```

Contextual keys: `:file` (required), `:specimen` (required), `:line-num` (optional), `:check` (optional)

### Skip ignores temporarily

```bash
proserunner document.md --skip-ignore
```

Useful for auditing all issues without filters.

### `--ignore` (deprecated)

`--ignore NAME` (or `-i`) never had an effect and now prints a warning. Use `--skip-ignore` for a run without ignores, or `.proserunnerignore` / `--exclude` to skip files.

## Project config

Set up project-specific settings:

```bash
proserunner init
```

Creates `.proserunner/` with `config.edn` and `checks/`.

### Config options

```clojure
{:check-sources ["default" "checks"]      ; Where to find checks
 :ignore #{"TODO" "FIXME"}                ; Ignore everywhere
 :ignore-issues [{:file "docs/guide.md"   ; Ignore at specific spots
                  :line 42
                  :specimen "very"}]
 :ignore-mode :extend                     ; :extend or :replace
 :config-mode :merged}                    ; :merged or :project-only
```

**check-sources:**

- `"default"` - Global checks (`~/.config/proserunner/default/`)
- `"checks"` - Project checks (`.proserunner/checks/`)
- Or any path (relative/absolute)

**ignore:**

- Strings to ignore everywhere
- Case-insensitive

**ignore-issues:**

- Specific file/line/specimen combos
- Auto-created by `--ignore-issues`

**ignore-mode:**

- `:extend` - Merge with global
- `:replace` - Project only

**config-mode:**

- `:merged` - Merge with global
- `:project-only` - Project only

### Use custom config temporarily

```bash
proserunner document.md --config /path/to/config.edn
```

Overrides both global and project configs for this run, including inside a project. Useful for testing different setups. The file must exist.

## Cache

Proserunner caches results for speed. Only re-checks files when content, config, or checks change.

**Location**, first match wins:

1. `--cache-dir DIR`
2. `$PROSERUNNER_CACHE_DIR`
3. `$XDG_CACHE_HOME/proserunner` (usually `~/.cache/proserunner`)
4. `$TMPDIR/proserunner-storage` (or the JVM temp dir if `TMPDIR` is unset)

**Clear cache:**

```bash
proserunner document.md --no-cache  # Recompute for this run
rm -rf ~/.cache/proserunner/        # Delete cache manually (adjust for your location)
```

**Cache invalidation triggers:**

- File content changes
- Config changes (enabled checks, settings)
- Check definitions change

## Custom checks

Add checks from a directory:

```bash
proserunner checks add ~/my-checks --global   # Global
proserunner checks add ~/my-checks --project  # Project
proserunner checks add ./checks --name style  # Custom name
```

Or drop `.edn` files in:

- Global: `~/.config/proserunner/custom/`
- Project: `.proserunner/checks/`

## Check examples

### Existence

```clojure
{:name "Writing tics"
 :kind "existence"
 :message "Stop using this phrase."
 :specimens ["phrase one" "phrase two"]}
```

### Case-sensitive

```clojure
{:name "Proper nouns"
 :kind "case"
 :message "Check proper noun usage."
 :specimens ["GitHub" "JavaScript"]}
```

### Recommender

```clojure
{:name "Terminology"
 :kind "recommender"
 :message "Use preferred terms."
 :recommendations [{:avoid "old term"
                    :prefer "new term"}]}
```

### Regex

```clojure
{:name "Custom patterns"
 :kind "regex"
 :expressions [{:re "\\b(very|really)\\b"
                :message "Avoid weak intensifiers."}]}
```

All need: `:name`, `:kind`, `:message`

## Custom editors

Write your own check types. Drop a Clojure file in `~/.config/proserunner/custom/`:

```clojure
;; my-editor.clj
(ns my-editor
  (:require [editors.registry :as registry]
            [proserunner.text :as text]))

(defn my-proofread [line check]
  ;; Return line with :issue? true and updated :issues vector
  ...)

(registry/register-editor! "my-check-type" my-proofread)
```

Use it:

```clojure
{:name "My Check"
 :kind "my-check-type"
 :message "Issue found."}
```

See `src/editors/` for examples.

## Reset default checks

```bash
proserunner checks restore
```

Backs up current, reinstalls the defaults, keeps your custom stuff.

Default checks are downloaded from GitHub once, on first run. After that, Proserunner never contacts the network on its own, so results don't change between runs. Run `checks restore` if you've edited or broken the defaults and want them back. See [Restore default checks](#restore-default-checks).

## Environment variables

| Variable | Effect |
| --- | --- |
| `XDG_CONFIG_HOME` | Global config goes in `$XDG_CONFIG_HOME/proserunner` (see [Config](#config)) |
| `PROSERUNNER_CACHE_DIR` | Cache directory (see [Cache](#cache)) |
| `XDG_CACHE_HOME` | Cache goes in `$XDG_CACHE_HOME/proserunner` |
| `TMPDIR` | Fallback cache location |
| `PROSERUNNER_DEBUG` | Any non-empty value prints error details and stack traces |
| `HTTPS_PROXY`, `ALL_PROXY`, `NO_PROXY` | Proxy for downloading the default checks |

## Upgrading from 0.7

Old flags keep working, but print a warning that names the replacement.

**Actions are commands now:**

| Old flag | Command |
| --- | --- |
| `--add-ignore X`, `-A X` | `proserunner ignore add X` |
| `--remove-ignore X`, `-R X` | `proserunner ignore remove X` |
| `--list-ignored`, `-L` | `proserunner ignore list` |
| `--clear-ignored`, `-X` | `proserunner ignore clear` |
| `--audit-ignores`, `-U` | `proserunner ignore audit` |
| `--clean-ignores`, `-W` | `proserunner ignore clean` |
| `--checks`, `-C` | `proserunner checks` |
| `--add-checks DIR`, `-a DIR` | `proserunner checks add DIR` |
| `--restore-defaults`, `-D` | `proserunner checks restore` |
| `--init-project`, `-I` | `proserunner init` |

`--ignore-issues` and `--ignore-all` are still options on a check, as before.

**Fewer single-letter flags.** Only `-c`, `-e`, `-f`, `-h`, `-o`, `-q`, and `-v` remain. The rest, such as `-b`, `-n`, `-d`, `-J`, `-G`, and `-P`, still work for now and warn. Use the long form: `--code-blocks`, `--no-cache`, `--cache-dir`, `--ignore-issues`, `--global`, `--project`.

**`-q` means `--quiet`.** It used to mean `--quoted-text`, which has no short form now. This is the one change that can't warn: a script that used `-q` to check quoted text now runs quietly and skips quoted text. Change it to `--quoted-text`.

**Config moved** from `~/.proserunner/` to `~/.config/proserunner/` (see [Config](#config)). The move happens on the first run. Backups from `checks restore` go in `backups/` there, not `~/.proserunner-backup-*`.

**Default checks are pinned** to the version each release was tested with; `checks restore` reinstalls that version instead of the latest.

## Performance baselines

Track performance, catch regressions.

### Create baseline

```bash
bb update-baseline  # Easiest

# Or manually
clojure -M:benchmark --save benchmark-baseline.edn
clojure -M:benchmark --editors-only --save baseline-editors.edn
```

### Compare

```bash
bb baseline  # Uses 10% threshold

# Custom threshold
clojure -M:benchmark --baseline benchmark-baseline.edn --threshold 15

# Editors only
clojure -M:benchmark --editors-only --baseline baseline-editors.edn
```

### Workflow

```bash
# Save before
clojure -M:benchmark --editors-only --save before.edn

# Make changes...

# Compare
clojure -M:benchmark --editors-only --baseline before.edn

# Update if better
bb update-baseline
```
