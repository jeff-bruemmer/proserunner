# P R O S E R U N N E R

Fast prose linter. Finds writing issues, lets you ignore what you don't care about.

## Example

```bash
$ proserunner document.md
```

```
document.md
[1]   3:13  "quick as lightning"  -> A tired phrase.
[2]   7:15  "software program"    -> Pleonastic phrase.
[3]  11:49  "hopefully"           -> Skunked term: consider rephrasing.
[4]  13:8   "Female booksalesman" -> Sexist or ridiculous term.
```

Fix what matters, ignore the rest by number:

```bash
$ proserunner document.md --ignore-issues 2,3
```

```
document.md
[1]   3:13  "quick as lightning"  -> A tired phrase.
[2]  13:8   "Female booksalesman" -> Sexist or ridiculous term.
```

## Quick start

```bash
proserunner document.md            # Check a file
proserunner docs/ README.md        # Check several paths
proserunner --init-project         # Set up project config
```

In CI, the exit status does the work: `0` means no issues, `1` means issues were found, `2` means something went wrong.

```bash
proserunner docs/ -o plain   # Fails the build on issues; one issue per line in the log
```

## Why it's useful

- **Numbered issues** - Ignore specific problems by number: `--ignore-issues 1,3,5`. Numbers are for the current run.
- **Customizable** - Add checks as EDN files. Toggle them on/off without touching code.
- **Fast** - Parallel processing, smart caching.
- **Flexible** - Output formats: grouped, plain (one issue per line), table, JSON, EDN, verbose.
- **Smart ignores** - Run it, fix what matters, ignore the rest by number. Ignores remember location.
- **Project friendly** - Commit ignore files and custom checks. Everyone sees the same issues.

## What it checks

Ships with 22 checks (run `proserunner --checks` to see yours):

| Name               | Kind        | Explanation                                                                                     |
| ------------------ | ----------- | ----------------------------------------------------------------------------------------------- |
| Annotations        | Case        | Typical markers used to signal problem sites in the text.                                       |
| Archaisms          | Existence   | Outmoded word or phrase.                                                                        |
| Clichés            | Existence   | An overused phrase or opinion that betrays a lack of original thought.                          |
| Compression        | Recommender | Compressing common phrases. From Style: Toward Clarity and Grace by Joseph M. Williams.         |
| Corporate-speak    | Existence   | Words and phrases that make you sound like an automaton.                                        |
| Hedging            | Existence   | Say, or say not. There is no hedging.                                                           |
| Jargon             | Existence   | Phrases infected with bureaucracy.                                                              |
| Meta-discourse     | Existence   | Self-referential phrases that add no value.                                                     |
| Needless-variant   | Recommender | Prefer the more common term.                                                                    |
| Nominalization     | Recommender | Convert weak noun phrases back to strong verbs.                                                 |
| Non-words          | Recommender | Identifies sequences of letters masquerading as words, and suggests an actual word.             |
| Not the negative.  | Recommender | Prefer the word to the negation of the word's opposite.                                         |
| Overused-adverbs   | Existence   | Use of adverbs that are weak or redundant. Consider using a stronger verb instead.              |
| Oxymorons          | Existence   | Avoid contradictory terms (that aren't funny).                                                  |
| Phrasal adjectives | Recommender | Hyphenate phrasal adjectives.                                                                   |
| Pompous-diction    | Recommender | Pompous diction: use simpler words. From Style: Toward Clarity and Grace by Joseph M. Williams. |
| Redundancies       | Existence   | Avoid phrases that say the same thing more than once.                                           |
| Regex              | Regex       | Raw regular expressions.                                                                        |
| Repetition         | Repetition  | Catches consecutive repetition of words, like _the the_.                                        |
| Sexism             | Existence   | Sexist or ridiculous terms (like _mail person_ instead of _mail carrier_).                      |
| Skunked-terms      | Existence   | Words with controversial correct usage that are best avoided.                                   |
| Weasel-words       | Existence   | Vague phrases that avoid commitment and precision.                                              |

[See check definitions →](https://github.com/jeff-bruemmer/proserunner-default-checks)

## Customizing checks

**Turn off checks** - Edit `~/.proserunner/config.edn`:

```clojure
{:checks
 [{:name "default"
   :directory "default"
   :files ["cliches"
           ;; "jargon"      ; disabled
           "corporate-speak"]}]}
```

**Add custom checks** - Drop `.edn` files in:

- Global: `~/.proserunner/custom/`
- Project: `.proserunner/checks/`

Or use: `proserunner --add-checks ~/my-checks`

See [Usage docs](docs/usage.md#custom-checks) for check examples.

## Docs

- [Installation](docs/installation.md) - Build and install
- [Usage](docs/usage.md) - Checks, config, ignores
- [Architecture](docs/architecture.md) - How it works
