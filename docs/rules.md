# Rules

A rule is a TOML file of words, patterns and terms to report, with a
message and examples. It can also turn on checks for what words can't
find. Its id is `<source>/<file name>`: the built-in `hedging.toml` is
`default/hedging`.

```toml
# .proserunner/rules/house-style.toml  →  id "project/house-style"
schema  = 1
message = "House style: “{suggestion}”, not “{match}”."
level   = "warning"
explain = "Our style guide, section 3."
scope   = ["paragraph", "list", "heading"]

terms = ["GitHub", { term = "macOS", match = ["mac os{, x}"] }]
words = [
  { match = "synerg{y,ies}", message = "Say what “{match}” means." },
  { match = "e-mail", suggest = ["email"] },
  { match = "utilize", suggest = ["use"], message = "Say “use”.", level = "hint" },
]
patterns = [
  { match = '!{2,}', message = "One exclamation mark is plenty." },
  { match = '(\d+) per ?cent', suggest = ["{1}%"] },
]

[examples]
bad  = ["Send an e-mail.", "Our synergies grew.", "Wow!!", "On Github.", "Up 5 per cent."]
good = ["Send an email.", "Wow!", "On GitHub, at github.com."]
fix  = [["Send an e-mail.", "Send an email."], ["Mac OS X is up 5 per cent.", "macOS is up 5%."]]
```

## Where rules come from

The config's `rules` list names the sources. `"default"` is the built-in
rules, and anything else is a directory whose `*.toml` files are rules:

```toml
rules = ["default", "~/writing/rules", { path = ".proserunner/rules", prefix = "project" }]
```

A directory's rules get ids that start with its name, or with `prefix`.
`proserunner rules export DIR [RULE…]` copies built-in rules to edit.
Use a copy in place of its built-in, or it reports each issue twice.

## Keys

| Key | Default | Meaning |
| --- | --- | --- |
| `schema` | (required) | The format version: `1` |
| `name` | from the file name | The name people see: `needless-variants.toml` is "Needless variants" |
| `message` | none | The message for each issue (see [Messages](#messages)) |
| `level` | `"warning"` | `"hint"`, `"warning"` or `"error"` |
| `explain` | none | Why the rule exists, for `explain` and verbose output |
| `case` | `"insensitive"` | `"insensitive"` or `"sensitive"` |
| `scope` | everywhere | Where the rule applies (see [Scope](#scope)) |
| `heading-levels` | every level | Which headings the rule applies to (see [Scope](#scope)) |
| `except` | none | Matches never to report (see [Except](#except)) |
| `words` | none | Literal words and phrases |
| `patterns` | none | Regular expressions |
| `terms` | none | Names written exactly one way (see [Terms](#terms)) |
| `repeat` | `false` | A word repeated right after itself (see [Repeat](#repeat)) |
| `consistent` | none | Spellings a document keeps to one of (see [Consistent](#consistent)) |
| `limit` | none | Sentences or blocks that are too long (see [Limit](#limit)) |
| `capitalization` | none | Words in the wrong case (see [Capitalization](#capitalization)) |
| `acronyms` | none | Acronyms used before they're defined (see [Acronyms](#acronyms)) |
| `metric` | none | Text that reads above a school grade (see [Metric](#metric)) |
| `examples` | none | `bad`, `good` and `fix` text for `rules test` |

A rule needs `words`, `patterns`, `terms` or a check. A rule file with a
mistake is reported, every mistake at once, and left out, and the other
rules still run.

## Words

A `words` entry is literal, so `etc.` and `C++` mean what they say.

- **Case:** with `case = "insensitive"`, any case matches, with Unicode
  case folding, so `über` matches `Über`.
- **Spaces:** a space matches any run of whitespace, line breaks
  included.
- **Word edges:** a word that starts or ends with a letter, digit or `_`
  matches only at a word's start or end. So `leverage` doesn't match
  `deleverage`, `known` doesn't match in `well-known`, and `github`
  doesn't match in `github.com`.
- **Braces:** `synerg{y,ies}` is `synergy` and `synergies`, and
  `thought leader{,s}` is `thought leader` and `thought leaders`. Braces
  without a comma are literal, `\{`, `\}`, `\,` and `\\` escape, and
  braces can't nest.

## Patterns

A `patterns` entry is a regular expression in
[Rust's syntax](https://docs.rs/regex/latest/regex/#syntax), used exactly
as written, so write `\b` where you want a word boundary. With
`case = "insensitive"`, `(?i)` goes in front.

Patterns match the text as the rules see it (see
[File formats](formats.md)): markup is gone, whitespace is one space, and
inline code is a placeholder (U+001A). Lookaround and backreferences
work, with a budget of 100,000 backtracking steps for each search: a
pattern that runs out stops matching in that block, with a warning.

`{1}` in a message or suggestion is the pattern's first group, and
`{year}` is a group named `(?<year>…)`.

## Terms

A term is a name with one right spelling. It matches in any case, and
each match not written exactly as the term is reported, with the term as
its suggestion: "Github" and "github" are issues, and "GitHub" isn't.

```toml
terms = [
  "GitHub",
  { term = "macOS", match = ["mac os{, x}", "osx"], level = "error" },
]
```

A table takes `term`, `match` (other spellings to find, with braces as
in `words`), `message` and `level`. Terms have the word edges `words`
have, and the longest match wins: with both "GitHub" and
"GitHub Actions" in `terms`, "Github actions" is one issue.

## Entries

A `words` or `patterns` entry can be a table:

```toml
{ match = "utilize", suggest = ["use", "employ"], message = "Simpler: {suggestion}.", level = "hint" }
```

`suggest` lists what to write instead, best first. `message`, `case`
and `level` replace the rule's for this entry, so one entry can match
`US` exactly in a rule that ignores case. A level in
[config](config.md) replaces the rule's and every entry's.

## Checks

A check finds what words and patterns can't. Each is a key of its own,
and a rule can turn on several:

```toml
# .proserunner/rules/consistency.toml
schema = 1
consistent = [["advisor{,s}", "adviser{,s}"], ["toward", "towards"]]
limit = { max = 30 }
acronyms = { except = ["HTML", "URL"] }
```

- **Scope:** a check looks only at the blocks its rule's `scope` and
  `heading-levels` cover.
- **`except`** drops a check's issues, and an excepted match doesn't
  count for `consistent` or `acronyms`.
- **Counts** are of the text the rules see: quoted text is masked, and
  inline code isn't a word. `it's`, `well-known`, `3.5` and `1,000` are
  one word each.

### Repeat

`repeat = true` reports a word repeated right after itself, in any case:
"the the", or "The the the", which is one issue. Only whitespace and
emphasis or block quote markers can come between them.

### Consistent

```toml
consistent = [["advisor{,s}", "adviser{,s}"], ["toward", "towards"], ["e-mail", "email"]]
```

Each group lists spellings that are each fine, as long as a document
keeps to one. The first a document uses wins, and each other form is
reported, with the winner as its suggestion. Forms match like `words`,
and each form in a group expands to as many spellings, so in a document
that writes "advisor" first, "advisers" suggests "advisors".

### Limit

```toml
limit = { max = 30, unit = "words", per = "sentence" }
```

Reports each sentence or block longer than `max`.

| Key | Default | Meaning |
| --- | --- | --- |
| `max` | (required) | The most allowed: 1 or more |
| `unit` | `"words"` | `"words"`, `"sentences"` (with `per = "block"`), or `"characters"` |
| `per` | `"sentence"` | `"sentence"`, or `"block"`: each paragraph, heading, list item or table cell |

A sentence ends at `.`, `?`, `!` or `…` before a space and a word that
doesn't start in lowercase, but not after an abbreviation or an
initial, such as `e.g.`, `Dr.` or `Fig. 3`.

### Capitalization

```toml
scope = ["heading"]
heading-levels = [1, 2]
capitalization = { style = "sentence", except = ["Proserunner", "Visual Studio Code"] }
```

Reports each word in the wrong case, with the right case as its
suggestion.

- **`style`:** `"sentence"`, the default, capitalizes the first word of
  each sentence. `"title"` capitalizes every word but articles,
  coordinating conjunctions and prepositions of four letters or fewer,
  unless first, last or after a colon.
- **Left alone:** words with a capital after the first letter (`GitHub`,
  `API`, `iOS`), `I`, the word after a colon in sentence case, and the
  word after inline code or quoted text that starts a sentence.
- **`except`:** words and phrases to leave as written, such as names.

### Acronyms

```toml
acronyms = { except = ["HTML", "URL", "NASA"] }
```

Reports an acronym's first use when it isn't defined by then. By
default, an acronym is three capital letters or more, plural or not
("APIs" is a use of "API"), and it's defined in parentheses after a
word: "application programming interface (API)". A file name such as
`README.md` isn't a use.

| Key | Default | Meaning |
| --- | --- | --- |
| `except` | none | Acronyms that need no definition |
| `match` | `'\b([A-Z]{3,})s?\b'` | A regex for an acronym where it's used. Its first group, if it has one, is the acronym |
| `definitions` | `['\w \(([^()\s]+?)s?\)']` | Regexes for a definition. The first group of each is the acronym it defines |

Both are case-sensitive. To accept "API (application programming
interface)" too:

```toml
acronyms = { definitions = ['\w \(([^()\s]+?)s?\)', '\b([A-Z]{3,})s? \([a-z]'] }
```

### Metric

```toml
metric = { formula = "flesch-kincaid", max = 10, per = "document" }
```

Reports text whose readability score, as a U.S. school grade, is over
`max`.

| Key | Default | Meaning |
| --- | --- | --- |
| `max` | (required) | The highest grade allowed |
| `formula` | `"flesch-kincaid"` | `"flesch-kincaid"`, `"gunning-fog"`, `"coleman-liau"` or `"automated-readability"` |
| `per` | `"document"` | `"document"`: every block the rule applies to, together. `"block"`: each block |
| `min-words` | `100` | Text with fewer words isn't scored |

## Messages

In a message, `{match}` is the match, `{suggestion}` the entry's
suggestions, and `{rule}` the rule's name. A pattern can use its groups
too. Suggestions take the same placeholders, but not `{suggestion}`.
`{{` and `}}` are literal braces. An entry with suggestions and no
message anywhere gets "Prefer: {suggestion}.", and any other needs one.
A missing message or a placeholder that can't be filled in is an error
when the rule loads, never a blank in the output.

A rule's `message` applies to its checks too. Each check has its own
placeholders and default:

| Check | Its placeholders | Default message |
| --- | --- | --- |
| `repeat` | none | Repeated word: “{match}”. |
| `consistent` | `{form}`: the spelling to use, also the suggestion | Prefer “{suggestion}”, which this document uses first. |
| `limit` | `{count}`, `{limit}` (the `max`), `{unit}` | {count} {unit}; keep it to {limit} or fewer. |
| `capitalization` | `{style}`, and `{form}`: the word in that style, also the suggestion | Use {style} case: “{suggestion}”. |
| `acronyms` | `{acronym}`: the acronym, without a plural `s` | Define “{acronym}” where it's first used, as in “Full Name ({acronym})”. |
| `metric` | `{score}`, `{limit}` (the `max`), `{formula}`: its name, such as Flesch-Kincaid | {formula} grade level {score}; aim for {limit} or lower. |

## Scope

| Scope | Blocks |
| --- | --- |
| `paragraph` | Paragraphs, footnotes, captions |
| `heading` | Headings and titles |
| `list` | List items |
| `table` | Table cells |
| `blockquote` | Block quotes |
| `code` | Code blocks, when checked with `--code-blocks` |

With no `scope`, a rule applies everywhere. `heading-levels` keeps it
to headings at some [levels](formats.md#heading-levels), where 0 is a
document's title and 1 its outermost sections:

```toml
scope = ["heading"]
heading-levels = [1, 2]
```

## Except

`except` lists matches this rule never reports. Each is text, or a
`pattern` that must match the whole match:

```toml
words  = ["leverage", "leverage partners", "leverage fund"]
except = ["Leverage Partners", { pattern = 'leverage (?:fund|partners)' }]
```

A rule reports its longest match, so "Leverage Partners" is dropped
while "leverage it" is reported. Without "leverage partners" in `words`,
the match would be "Leverage", and `except` wouldn't apply. To accept a
word for every rule, use `accept` in [config](config.md).

## Examples and `rules test`

```console
$ proserunner rules test                    # Every rule the config loads
$ proserunner rules test .proserunner/rules # Only the rules in a directory
```

Examples are Markdown, with quoted text and code checked, so a `heading`
rule's example starts with `#`. `rules test` checks that each `bad`
example gets an issue from its rule, each `good` one gets none, and each
`fix` pair `[before, after]` holds when each issue takes its first
suggestion, in the case of what it replaces (a term, or a suggestion
that changes only case, keeps its own). A rule with no `bad`
example fails too. The built-in rules' examples run in `cargo test`.

To see what else a rule matches, run it on your documents with `--rule`,
and add `--show-suppressed` to see what hides its issues.

## `explain`

`proserunner explain RULE` shows a rule's level and where it was set,
each entry and check as it compiled, its examples, and every `except`,
`accept`, ignore and override that applies to it.
