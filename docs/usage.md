# Usage

Everything you need to use Proserunner.

## Quick reference

```bash
# Check files - issues get numbers
proserunner --file /path/to/file
# Output:
# document.md
# [1]  10:5   "utilize"  -> Consider using "use" instead.
# [2]  15:12  "leverage" -> Consider using "use" instead.

# Ignore by number
proserunner --file document.md --ignore-issues 1,3
proserunner --file document.md --ignore-issues 1-3,5  # Ranges work too

# Ignore everything currently showing
proserunner --file document.md --ignore-all
proserunner --file document.md --ignore-all --global  # Force global scope

# Ignore a word everywhere
proserunner --add-ignore "hopefully"

# Clean up old ignores
proserunner --audit-ignores
proserunner --clean-ignores

# Check quoted dialogue too
proserunner --file document.md --quoted-text

# Skip files/directories
proserunner --file docs/ --exclude "drafts/*"
proserunner --file docs/ --exclude "drafts/*,*.backup,temp.md"  # Comma-separated
```

## Default checks

Ships with 18 checks. See what's enabled: `proserunner --checks`

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
proserunner --file document.md --quoted-text
```

Works with straight quotes (`"..."`, `'...'`) and curly quotes (`"..."`, `'...'`).

## Config

First run downloads checks to `~/.proserunner/`:

```
~/.proserunner/
├── config.edn      # Main config
├── ignore.edn      # Global ignores
├── custom/         # Your checks
└── default/        # Default checks
```

### Turn off checks

Edit `~/.proserunner/config.edn`:

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
proserunner --add-ignore "hopefully"
proserunner --remove-ignore "hopefully"
proserunner --list-ignored
```

### Contextual ignores

Ignore by issue number:

```bash
proserunner --file document.md
# [1]  10:5   "utilize"  -> Consider using "use" instead.
# [2]  15:12  "leverage" -> Consider using "use" instead.

proserunner --file document.md --ignore-issues 1,2    # Ignore 1 and 2
proserunner --file document.md --ignore-issues 1-5,8  # Ranges work
proserunner --file document.md --ignore-all           # Ignore everything shown
```

Issue numbers are only valid for the current run. The system stores the actual location (file:line:col:specimen), so the next run will renumber remaining issues.

**Scope:**

- Default: project if `.proserunner/` exists, else global
- Force with `--global` or `--project`

### Clean up ignores

```bash
proserunner --audit-ignores  # Find stale ones
proserunner --clean-ignores  # Remove them
```

### Edit manually

Global (`~/.proserunner/ignore.edn`):

```clojure
["hopefully"  ; Simple
 {:file "docs/api.md" :line-num 42 :specimen "utilize"}]  ; Contextual
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
proserunner --file document.md --skip-ignore
```

Useful for auditing all issues without filters.

### Custom ignore file

```bash
proserunner --ignore my-ignores  # Use different ignore file name
```

Default is `ignore.edn` - this lets you use different names.

## Project config

Set up project-specific settings:

```bash
proserunner --init-project
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

- `"default"` - Global checks (`~/.proserunner/default/`)
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
proserunner --file document.md --config /path/to/config.edn
```

Overrides both global and project configs for this run. Useful for testing different setups.

## Cache

Proserunner caches results for speed. Only re-checks files when content, config, or checks change.

**Location:** `~/.proserunner/cache/`

**Clear cache:**

```bash
proserunner --file document.md --no-cache  # Skip cache for this run
rm -rf ~/.proserunner/cache/               # Delete cache manually
```

**Cache invalidation triggers:**

- File content changes
- Config changes (enabled checks, settings)
- Check definitions change

## Custom checks

Add checks from a directory:

```bash
proserunner --add-checks ~/my-checks --global   # Global
proserunner --add-checks ~/my-checks --project  # Project
proserunner --add-checks ./checks --name style  # Custom name
```

Or drop `.edn` files in:

- Global: `~/.proserunner/custom/`
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

Write your own check types. Drop a Clojure file in `~/.proserunner/custom/`:

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

## Reset to defaults

```bash
proserunner --restore-defaults
```

Backs up current, downloads fresh defaults, keeps your custom stuff.

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
