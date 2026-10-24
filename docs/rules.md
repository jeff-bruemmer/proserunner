# Rules

A rule is a TOML file of words, patterns and terms to report, with a
message and examples. It can also turn on checks for what words can't
find: mixed spellings, long sentences, headings in the wrong case,
acronyms used before they're defined, and text too hard to read. Its id
is `<source>/<file name>`: the built-in `hedging.toml` is
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
rules. Anything else is a directory, and every `*.toml` file directly in
it is a rule:

```toml
rules = ["default", "~/writing/rules", { path = ".proserunner/rules", prefix = "project" }]
```

A directory's rules get ids that start with the directory's name, or
with `prefix` when you give one. `proserunner init` sets up
`.proserunner/rules/` with the prefix `project`. To start from the
built-ins, `proserunner rules export DIR` copies them into `DIR`.

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
| `heading-levels` | every level | Which headings the rule applies to (see [Heading levels](#heading-levels)) |
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

A rule needs `words`, `patterns`, `terms` or a check. An unknown key, an unknown `scope` or `level`, an invalid pattern, or a
message that can't be filled in is an error that names the file (and the
line and column, for a TOML mistake). A rule file with an error is left
out of the run with a warning, and the other rules still run.

## Words

A `words` entry is a literal string. Nothing in it is a regular
expression, so `etc.` and `C++` mean exactly what they say.

- **Case:** with `case = "insensitive"`, any case matches, with Unicode
  case folding, so `über` matches `Über`.
- **Spaces:** a space matches any run of whitespace, line breaks
  included.
- **Word edges:** a word that starts with a letter, digit or `_` matches
  only at the start of a word, and one that ends with one matches only at
  the end of a word. So `leverage` doesn't match `deleverage`, and `etc.`
  matches before any character. A word doesn't start right after a
  hyphen, so `known` doesn't match in `well-known`. Nor is it part of a
  dotted name: `github` doesn't match in `github.com` or
  `docs.github.io`, nor `main` in `main.rs`, though it still matches
  before a period that ends a sentence.
- **Braces:** `{a,b}` expands into each choice: `synerg{y,ies}` is
  `synergy` and `synergies`, and `thought leader{,s}` is
  `thought leader` and `thought leaders`. Braces without a comma are
  literal, and `\{`, `\}`, `\,` and `\\` escape. An entry can expand into
  1,000 words at most, and braces can't nest.

## Patterns

A `patterns` entry is a regular expression in
[Rust's syntax](https://docs.rs/regex/latest/regex/#syntax), used exactly
as written. Nothing is added, so write `\b` where you want a word
boundary. With `case = "insensitive"`, `(?i)` goes in front.

Patterns match the text as the rules see it (see
[File formats](formats.md)): markup is gone, whitespace is one space, and
inline code is a placeholder (U+001A).

Lookaround and backreferences work. Patterns that use them run on a
backtracking engine, with a budget of 100,000 steps for each search. A
pattern that runs out stops matching in that block, so it can't stall a
run, and proserunner warns with the file, line and rule, since it may
have missed issues there. Every other pattern runs in linear time.

A pattern's groups can fill in its message and suggestions: `{1}` is
the first group, `{2}` the second, and a named group `(?<year>…)` is
also `{year}`. A group that takes no part in a match is empty. A group
can't be named `match`, `suggestion` or `rule`.

```toml
patterns = [{ match = '(\d+) per ?cent', suggest = ["{1}%"] }]
```

## Terms

A term is a name with one right spelling. It matches in any case, and
every match not written exactly as the term is reported, with the term
as its suggestion: "Github", "GITHUB" and "github" are issues, and
"GitHub" isn't.

```toml
terms = [
  "GitHub",
  "JavaScript",
  { term = "macOS", match = ["mac os{, x}", "osx"], level = "error" },
]
```

- **`match`:** other spellings to find, in any case. Braces expand as in
  `words`; the term itself is literal.
- **Edges:** terms have the word edges `words` have, so `github.com` is
  left alone.
- **Longest match:** as with words, the longest match wins, and a term
  written right still counts as a match. With both "GitHub" and
  "GitHub Actions" in `terms`, "GitHub Actions" reports nothing, and
  "Github actions" is one issue.
- **Table keys:** `term`, `match`, `message` and `level`. Without a
  message anywhere, an issue says "Prefer: {suggestion}.".
- **`fix` examples:** a term replaces a match exactly as the term is
  written, never in the case of the match.

## Entries

Any `words` or `patterns` entry can be a table instead of a string:

```toml
{ match = "utilize", suggest = ["use", "employ"], message = "Simpler: {suggestion}.", level = "hint" }
```

- **`match`:** the word or pattern.
- **`suggest`:** what to write instead, best first. Suggestions can use
  placeholders as messages do (see [Messages](#messages)).
- **`message`:** the message for this entry's issues, in place of the
  rule's.
- **`case`:** `"insensitive"` or `"sensitive"`, in place of the rule's
  `case`, so one entry can match `US` exactly in a rule that ignores
  case.
- **`level`:** how serious this entry's issues are, in place of the
  rule's `level`. A rule's level in [config](config.md) replaces its own
  and every entry's.

## Checks

Words, patterns and terms match text. A check finds what they can't.
Each is a key of its own, and a rule can turn on several, with or
without entries:

```toml
# .proserunner/rules/consistency.toml
schema = 1
consistent = [["advisor{,s}", "adviser{,s}"], ["toward", "towards"]]
limit = { max = 30 }
acronyms = { except = ["HTML", "URL"] }
```

- **Scope:** a check looks only at the blocks its rule's `scope` and
  `heading-levels` cover.
- **Overlaps:** a rule's matches hide each other only among its
  entries, or among one check's issues. A sentence over a `limit`
  doesn't hide a word in it.
- **Except and accept:** `except` drops a check's issues by their whole
  match, as it does entries'. For `consistent` and `acronyms`, an
  excepted match doesn't count either: it can't be the spelling that
  wins, or an acronym's use or definition. `accept` and ignore lists
  hide issues after that.
- **Messages:** each check has a default message and placeholders of its
  own (see [Messages](#messages)). A rule's `message` applies to its
  checks too, so it has to suit each of them.
- **One pass:** what `consistent` and `acronyms` look for is found in
  the same pass as every rule's words and patterns.
- **Counts** are of the text the rules see: quoted text is masked
  unless you pass `--quoted-text`, and inline code is a placeholder,
  not a word. A word with an apostrophe or a hyphen inside is one word
  (`it's`, `well-known`), and so is a number with a decimal point or
  commas (`3.5`, `1,000`).

### Repeat

`repeat = true` reports a word repeated right after itself, ignoring
case: "the the", "is is". The words must be separated only by
whitespace, or by emphasis (`*`, `_`, `~`) or block quote (`>`) markers.
"The the the" is one issue.

### Consistent

```toml
consistent = [["advisor{,s}", "adviser{,s}"], ["toward", "towards"], ["e-mail", "email"]]
```

Each group lists spellings that are each fine, as long as a document
keeps to one. The first a document uses wins, and each use of another
form in its group is reported, with the winner as its suggestion.

- **Matching:** each form matches like a `words` entry: literally, on
  word edges, with the rule's `case`. Forms that differ only in case
  need `case = "sensitive"`.
- **Braces:** braces expand as in `words`, and every form in a group
  must expand to as many spellings, so each spelling has one to
  suggest: in a document that writes "advisor" first, "advisers"
  suggests "advisors".
- **First:** first in the document, across every block the rule applies
  to.

### Limit

```toml
limit = { max = 30, unit = "words", per = "sentence" }
```

Reports each sentence or block longer than `max`, all of it.

| Key | Default | Meaning |
| --- | --- | --- |
| `max` | (required) | The most allowed: 1 or more |
| `unit` | `"words"` | `"words"`, `"sentences"`, or `"characters"` as the rules see them, spaces included, with hidden text such as a URL as one |
| `per` | `"sentence"` | `"sentence"`, or `"block"`: each paragraph, heading, list item or table cell |

A sentence ends at `.`, `?`, `!` or `…` when a space follows and the
next word doesn't start in lowercase. A period doesn't end one after an
abbreviation such as `e.g.`, a title such as `Dr.`, an initial, or an
abbreviation before a number (`Fig. 3`). `unit = "sentences"` needs
`per = "block"`. To keep headings to eight words:

```toml
scope = ["heading"]
limit = { max = 8, per = "block" }
```

### Capitalization

```toml
scope = ["heading"]
heading-levels = [1, 2]
capitalization = { style = "sentence", except = ["Proserunner", "Visual Studio Code"] }
```

Reports each word that the style writes in another case, with the word
in that case as its suggestion. Most rules with it set
`scope = ["heading"]`.

- **`style`:** `"sentence"`, the default, capitalizes the first word of
  each sentence, and nothing else. `"title"` capitalizes every word but
  articles, coordinating conjunctions and prepositions of four letters
  or fewer: a, an, and, as, at, but, by, for, from, in, into, nor, of,
  off, on, onto, or, out, over, per, so, the, to, up, upon, via, vs,
  with and yet. Those are lowercase, unless first, last, or after a
  colon.
- **Left alone:** a word with a capital after its first letter
  (`GitHub`, `API`, `iOS`), `I` and its contractions, a word that
  doesn't start with a letter, and in sentence case, the word after a
  colon.
- **After hidden text:** inline code or quoted text before a sentence's
  first word may itself be the first, so the word after it may be in
  either case: "install" in "`npm` install steps", and "Start" in
  "“Quick” Start". In title case, that holds for the short words above;
  other words are capitalized anyway.
- **`except`:** words and phrases to leave as written, such as names,
  matched exactly and as whole words.
- **Issues:** each word is an issue of its own, so `accept` in config
  and ignore lists work on single words, as for any rule.

### Acronyms

```toml
acronyms = { except = ["HTML", "URL", "NASA"] }
```

Reports an acronym's first use when it isn't defined by then, and only
its first. By default, an acronym is three capital letters or more,
with a plural `s` or without ("APIs" is a use of "API"), and it's defined
in parentheses after a word: "application programming interface
(API)". A use inside its definition counts as defined, and one joined
to a word by a period, as in a file name such as `README.md`, isn't a
use.

| Key | Default | Meaning |
| --- | --- | --- |
| `except` | none | Acronyms that need no definition |
| `match` | `'\b([A-Z]{3,})s?\b'` | A regex for an acronym where it's used. Its first group, if it has one, is the acronym |
| `definitions` | `['\w \(([^()\s]+?)s?\)']` | Regexes for where an acronym is defined. The first group of each is the acronym it defines, and the definition counts from where its match starts |

Both are matched with case, whatever the rule's `case`, and see text as
patterns do, with whitespace collapsed to one space. A use or a
definition outside the rule's `scope` doesn't count. To accept
"API (application programming interface)" too:

```toml
acronyms = { definitions = ['\w \(([^()\s]+?)s?\)', '\b([A-Z]{3,})s? \([a-z]'] }
```

### Metric

```toml
metric = { formula = "flesch-kincaid", max = 10, per = "document" }
```

Scores text by a readability formula, as a U.S. school grade, and
reports it when the score is over `max`.

| Key | Default | Meaning |
| --- | --- | --- |
| `max` | (required) | The highest grade allowed |
| `formula` | `"flesch-kincaid"` | One of the formulas below |
| `per` | `"document"` | `"document"`: every block the rule applies to, together, reported at the first one's first sentence. `"block"`: each block, reported over all of it |
| `min-words` | `100` | Text with fewer words isn't scored, since the formulas mean little for less |

| Formula | Grade |
| --- | --- |
| `flesch-kincaid` | 0.39 × words per sentence + 11.8 × syllables per word − 15.59 |
| `gunning-fog` | 0.4 × (words per sentence + 100 × the share of words with three syllables or more) |
| `coleman-liau` | 0.0588 × letters per 100 words − 0.296 × sentences per 100 words − 15.8 |
| `automated-readability` | 4.71 × letters per word + 0.5 × words per sentence − 21.43 |

Letters include digits. Syllables are counted from runs of vowels, less
a silent final `e`, `-ed` or `-es`, which is right for most English
words. A score is compared with `max` rounded to one decimal place, as
messages show it.

## Messages

`{match}` in a message stands for the match, `{suggestion}` for the
entry's suggestions (separated by commas), and `{rule}` for the rule's
name. A pattern's messages can use its groups too (see
[Patterns](#patterns)). Suggestions are filled in the same way, except
that they can't use `{suggestion}`. `{{` and `}}` are literal braces.

- **Defaults:** an entry with suggestions and no message anywhere gets
  "Prefer: {suggestion}.", and each check has its own (below).
- **Mistakes:** each of these is an error when the rule loads, never a
  blank in the output:
  - an unknown placeholder (the error names the closest known one, or
    the groups the pattern has);
  - `{suggestion}` in a message for an entry with no `suggest`, or in a
    suggestion;
  - an entry with no message to use;
  - a mistake in the rule's `message`, even when every entry has its
    own.

Checks fill placeholders of their own. Only `consistent` and
`capitalization` suggest anything, so only their messages can use
`{suggestion}`.

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

With no `scope`, a rule applies everywhere.

### Heading levels

`heading-levels` keeps a rule to headings at some levels: 0 is a
document's title, 1 its outermost sections, and 2 the sections inside
those (see [File formats](formats.md#heading-levels) for each format).
Other blocks are left to `scope`:

```toml
scope = ["heading"]
heading-levels = [1, 2]  # Only the top two levels of headings
```

With no `scope`, such a rule checks all prose but headings at other
levels. With a `scope` that leaves out `"heading"`, `heading-levels`
would do nothing, so it's an error.

## Except

`except` lists matches that this rule never reports. Each is compared with
a whole match, never a part of one, using the rule's case. An item is
text, or a table with a `pattern`: a regular expression that must match
the whole match.

```toml
words  = ["leverage", "leverage partners", "leverage fund"]
except = ["Leverage Partners", { pattern = 'leverage (?:fund|partners)' }]
```

A rule reports the longest match it finds, so in "ask Leverage Partners"
the match is "Leverage Partners", which `except` drops, while "leverage
it" is still reported. Without "leverage partners" in `words`, the match
would be "Leverage" and `except` wouldn't apply.

To accept a word for every rule, or only in your project, use `accept` in
[config](config.md) instead.

## Examples and `rules test`

```console
$ proserunner rules test                    # Every rule the config loads
$ proserunner rules test .proserunner/rules # Only the rules in a directory
```

Examples are Markdown, read with quoted text and code blocks checked, so
a `code` rule's example is a ```` ``` ```` fence and a `heading` rule's
starts with `#`. A bad example with no text where its rule applies says
so. For every rule, it checks that:

- each `bad` example gets at least one issue from the rule;
- each `good` example gets none;
- each `fix` pair `[before, after]` holds: replacing each issue in
  `before` with its first suggestion, in the case of what it replaces,
  gives `after`. A term keeps its own case, and so does a suggestion
  that changes only case, such as `capitalization`'s. Where issues
  overlap, the first is replaced.

A rule with no `bad` example is reported too, since nothing shows that it
matches. Problems are printed one per line, and the exit status is 1 when
there are any. The built-in rules' examples run in `cargo test`.

## `explain`

`proserunner explain RULE` takes an id, or a rule's name alone when only
one rule has it. It shows:

- the rule's level, and the config file that set it;
- its source, case, scope and message;
- each entry as it compiled: a literal or a regular expression, its word
  edges, and whether it backtracks, with a term's other spellings, and
  the entry's own case and level where they differ from the rule's;
- each check, with its settings and defaults filled in;
- the examples;
- every `except`, `accept` and ignore that keeps the rule's issues from
  being reported, with the file it's in;
- the `[[overrides]]` that change the rule.
