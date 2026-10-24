//! Words, sentences and syllables in a block's matching view. Pure, and
//! without language models: what ends a sentence is decided from the
//! characters around it, and syllables from runs of vowels.

use std::iter::Peekable;
use std::ops::{Add, AddAssign, Range};
use std::str::CharIndices;

use crate::scan::PLACEHOLDER;

/// Titles that come before a name, lowercased: a period after one never
/// ends a sentence.
const TITLES: &[&str] = &["dr", "mr", "mrs", "ms", "mx", "mt", "prof", "rev", "st"];

/// Abbreviations that come before a number, lowercased: a period after
/// one doesn't end a sentence when a digit follows (`Fig. 3`).
const BEFORE_NUMBERS: &[&str] = &[
    "approx", "ca", "ch", "eq", "eqs", "fig", "figs", "no", "nos", "p", "pp", "sec", "vol", "vols",
];

/// Lowercase accented vowels.
const ACCENTED_VOWELS: &str = "àáâäæèéêëìíîïòóôöœùúûü";

/// Word ranges: runs of word characters. An apostrophe or hyphen inside a
/// word is part of it (`it's`, `well-known`), and so is a period or comma
/// between digits (`3.5`, `1,000`).
pub fn words(text: &str) -> Words<'_> {
    Words { text, at: 0 }
}

/// The word ranges of a text, in order: see [`words`].
#[derive(Debug, Clone)]
#[must_use = "iterators are lazy and do nothing unless consumed"]
pub struct Words<'text> {
    text: &'text str,
    /// Where the next word may start.
    at: usize,
}

impl Iterator for Words<'_> {
    type Item = Range<usize>;

    fn next(&mut self) -> Option<Range<usize>> {
        let text = self.text;
        let start = loop {
            let (c, len) = char_at(text, self.at)?;
            if is_word_char(c) {
                break self.at;
            }
            self.at += len;
        };
        let mut prev = ' ';
        while let Some((c, len)) = char_at(text, self.at) {
            let joins = || {
                let next = char_at(text, self.at + len).map_or(' ', |(n, _)| n);
                match c {
                    // U+2019 right single quote, U+2010 hyphen, U+2011 non-breaking hyphen.
                    '\'' | '\u{2019}' | '-' | '\u{2010}' | '\u{2011}' => is_word_char(next),
                    '.' | ',' => prev.is_ascii_digit() && next.is_ascii_digit(),
                    _ => false,
                }
            };
            if !is_word_char(c) && !joins() {
                break;
            }
            prev = c;
            self.at += len;
        }
        Some(start..self.at)
    }
}

/// The character at byte `i` of `s`, and its length, reading ASCII
/// without decoding.
fn char_at(s: &str, i: usize) -> Option<(char, usize)> {
    let b = *s.as_bytes().get(i)?;
    if b.is_ascii() {
        return Some((char::from(b), 1));
    }
    let c = s.get(i..)?.chars().next()?;
    Some((c, c.len_utf8()))
}

/// What readability formulas count in a text.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Counts {
    /// As [`sentences`] splits them.
    pub sentences: usize,
    /// As [`words`] splits them.
    pub words: usize,
    /// In every word, as [`syllables`] estimates them.
    pub syllables: usize,
    /// Words of three syllables or more.
    pub polysyllables: usize,
    /// Letters and digits in words.
    pub characters: usize,
}

impl Counts {
    /// The counts for `text`, in one pass over its words.
    #[must_use]
    pub fn of(text: &str) -> Counts {
        let mut counts = Counts {
            sentences: sentences(text).count(),
            ..Counts::default()
        };
        // One pass over each word for syllables and letters.
        for range in words(text) {
            let word = &text[range];
            let n = syllables(word);
            counts.words += 1;
            counts.syllables += n;
            counts.polysyllables += usize::from(n >= 3);
            counts.characters += word.chars().filter(|c| c.is_alphanumeric()).count();
        }
        counts
    }
}

impl Add for Counts {
    type Output = Counts;

    fn add(self, other: Counts) -> Counts {
        Counts {
            sentences: self.sentences + other.sentences,
            words: self.words + other.words,
            syllables: self.syllables + other.syllables,
            polysyllables: self.polysyllables + other.polysyllables,
            characters: self.characters + other.characters,
        }
    }
}

impl AddAssign for Counts {
    fn add_assign(&mut self, other: Counts) {
        *self = *self + other;
    }
}

/// About how many syllables `word` has in English: one per run of vowels,
/// less a silent final `e`, `-ed` or `-es`. A hyphenated word is the sum of
/// its parts, and a number is one syllable.
pub fn syllables(word: &str) -> usize {
    // U+2010 hyphen, U+2011 non-breaking hyphen.
    word.split(['-', '\u{2010}', '\u{2011}'])
        .map(part_syllables)
        .sum::<usize>()
        .max(1)
}

fn part_syllables(part: &str) -> usize {
    // The last four letters, lowercase, for the ending.
    let mut tail = [' '; 4];
    let mut letters = 0;
    let mut count = 0;
    let mut in_vowels = false;
    let alphabetic = part.chars().filter(|c| c.is_alphabetic());
    for c in alphabetic {
        let c = lowercase(c);
        // A leading `y` is a consonant: "yes", but "gym".
        let vowel = is_vowel(c) || (c == 'y' && letters > 0);
        if vowel && !in_vowels {
            count += 1;
        }
        in_vowels = vowel;
        tail = [tail[1], tail[2], tail[3], c];
        letters += 1;
    }
    if letters == 0 {
        return usize::from(part.chars().any(|c| c.is_numeric()));
    }
    if count > 1 && silent_ending(&tail[4 - letters.min(4)..]) {
        count -= 1;
    }
    count.max(1)
}

fn lowercase(c: char) -> char {
    if c.is_ascii() {
        c.to_ascii_lowercase()
    } else {
        c.to_lowercase().next().unwrap_or(c)
    }
}

fn is_vowel(c: char) -> bool {
    matches!(c, 'a' | 'e' | 'i' | 'o' | 'u') || (!c.is_ascii() && ACCENTED_VOWELS.contains(c))
}

/// Whether a lowercase word ends in a vowel that isn't heard: "make",
/// "jumped", "makes", but not "table", "wanted" or "boxes".
fn silent_ending(w: &[char]) -> bool {
    let consonant = |c: char| !is_vowel(c) && c != 'y';
    match *w {
        [.., b, 'l', 'e'] => !consonant(b),
        [.., 'e', 'e'] => false,
        [.., b, 'e'] => consonant(b),
        [.., b, 'e', 'd'] => consonant(b) && !matches!(b, 't' | 'd'),
        [.., a, b, 'e', 's'] => {
            consonant(b)
                && !matches!(b, 's' | 'x' | 'z' | 'g' | 'c')
                && !matches!((a, b), ('c' | 's', 'h'))
        }
        _ => false,
    }
}

/// Sentence ranges, without the space around them. A stretch with
/// nothing but hidden text, such as a quotation on its own, isn't one.
///
/// A sentence ends at a run of `.`, `?`, `!` or `…`, with any closing
/// quotes or brackets after it, when space follows and the next word
/// doesn't start with a lowercase letter. A period doesn't end one after
/// an abbreviation with periods inside (`e.g.`, `U.S.`), an initial
/// (`J.`), a title (`Dr.`), or an abbreviation before a number
/// (`Fig. 3`).
pub fn sentences(text: &str) -> Sentences<'_> {
    Sentences {
        text,
        chars: text.char_indices().peekable(),
        start: 0,
    }
}

/// The sentence ranges of a text, in order: see [`sentences`].
#[derive(Debug, Clone)]
#[must_use = "iterators are lazy and do nothing unless consumed"]
pub struct Sentences<'text> {
    text: &'text str,
    chars: Peekable<CharIndices<'text>>,
    /// Where the next sentence starts, with the space before it.
    start: usize,
}

impl Iterator for Sentences<'_> {
    type Item = Range<usize>;

    fn next(&mut self) -> Option<Range<usize>> {
        let text = self.text;
        while let Some((i, c)) = self.chars.next() {
            if !is_terminal(c) {
                continue;
            }
            let mut end = i + c.len_utf8();
            while let Some(&(j, d)) = self.chars.peek() {
                if !is_terminal(d) && !is_closer(d) {
                    break;
                }
                end = j + d.len_utf8();
                self.chars.next();
            }
            if ends_sentence(text, i, c, end) {
                let sentence = non_blank(text, self.start..end);
                self.start = end;
                if sentence.is_some() {
                    return sentence;
                }
            }
        }
        let rest = non_blank(text, self.start..text.len());
        self.start = text.len();
        rest
    }
}

fn is_terminal(c: char) -> bool {
    matches!(c, '.' | '?' | '!' | '…')
}

fn is_closer(c: char) -> bool {
    // U+201D right double quote, U+2019 right single quote.
    matches!(c, '"' | '\'' | '\u{201D}' | '\u{2019}' | '»' | ')' | ']')
}

/// Whether the marks from `mark` (the first, `c`) to `end` end a sentence.
fn ends_sentence(text: &str, mark: usize, c: char, end: usize) -> bool {
    let rest = &text[end..];
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return false;
    }
    let Some(next) = rest.trim_start().chars().next() else {
        return true;
    };
    if next.is_lowercase() {
        return false;
    }
    // Only a lone period can follow an abbreviation: `?`, `...` or a
    // closing quote ends the sentence whatever comes before.
    if c != '.' || end > mark + 1 {
        return true;
    }
    let word = text[..mark].rsplit(char::is_whitespace).next();
    !abbreviation(word.unwrap_or_default(), next)
}

/// Whether `word`, just before a period, is an abbreviation when `next`
/// starts the following word.
fn abbreviation(word: &str, next: char) -> bool {
    let word = word.trim_start_matches(|c: char| !c.is_alphanumeric());
    let mut letters = word.chars();
    let initial = letters.next().is_some_and(char::is_uppercase) && letters.next().is_none();
    // The lists are ASCII, so ASCII case folding is enough.
    let listed = |list: &[&str]| list.iter().any(|t| word.eq_ignore_ascii_case(t));
    dotted(word) || initial || listed(TITLES) || (next.is_ascii_digit() && listed(BEFORE_NUMBERS))
}

/// Whether `word` is letters in parts of one or two, joined by periods:
/// `e.g`, `U.S`, `Ph.D`, but not `3.5` or `main.rs`.
fn dotted(word: &str) -> bool {
    word.contains('.')
        && word.split('.').all(|part| {
            (1..=2).contains(&part.chars().count()) && part.chars().all(char::is_alphabetic)
        })
}

/// `range` trimmed, or `None` when it has no prose: only whitespace and
/// placeholders.
fn non_blank(text: &str, range: Range<usize>) -> Option<Range<usize>> {
    let range = trimmed(text, range);
    text[range.clone()]
        .chars()
        .any(|c| !c.is_whitespace() && c != PLACEHOLDER)
        .then_some(range)
}

/// Whether `c` is a letter, digit or `_`, which make up words and decide
/// where their boundaries are.
pub fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Collapses runs of whitespace to one space, the way the matching view of
/// a document does, so entries and matches compare alike.
pub fn normalize(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `range` of `text` without the space around it.
pub(crate) fn trimmed(text: &str, range: Range<usize>) -> Range<usize> {
    let s = &text[range.clone()];
    let start = range.start + (s.len() - s.trim_start().len());
    start..(range.start + s.trim_end().len()).max(start)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split(text: &str) -> Vec<&str> {
        sentences(text).map(|r| &text[r]).collect()
    }

    #[test]
    fn sentence_cases() {
        let cases: &[(&str, &[&str])] = &[
            ("", &[]),
            ("   ", &[]),
            ("No ending", &["No ending"]),
            ("One. Two.", &["One.", "Two."]),
            ("Is it? Yes! Done.", &["Is it?", "Yes!", "Done."]),
            ("  Spaced.   Out.  ", &["Spaced.", "Out."]),
            ("Really?! Yes.", &["Really?!", "Yes."]),
            // Lowercase after the mark: the sentence goes on.
            ("Wait... then go.", &["Wait... then go."]),
            ("Is it ok? yes it is.", &["Is it ok? yes it is."]),
            ("Wait… Then go.", &["Wait…", "Then go."]),
            // No space after the mark: a number, a file name.
            (
                "Version 2.0 is out. Get it.",
                &["Version 2.0 is out.", "Get it."],
            ),
            ("Open main.rs now.", &["Open main.rs now."]),
            // Closers stay with their sentence.
            ("(It ended.) Then more.", &["(It ended.)", "Then more."]),
            ("“Stop.” She left.", &["“Stop.”", "She left."]),
            (
                "He said \"no.\" Then left.",
                &["He said \"no.\"", "Then left."],
            ),
            // Abbreviations.
            (
                "Use e.g. Python here. Next.",
                &["Use e.g. Python here.", "Next."],
            ),
            (
                "The U.S. Army left. Then.",
                &["The U.S. Army left.", "Then."],
            ),
            (
                "Dr. Smith arrived. He sat.",
                &["Dr. Smith arrived.", "He sat."],
            ),
            ("(Mr. Li) came. Bye.", &["(Mr. Li) came.", "Bye."]),
            (
                "J. R. R. Tolkien wrote it. Then.",
                &["J. R. R. Tolkien wrote it.", "Then."],
            ),
            (
                "See Fig. 3 for more. Next.",
                &["See Fig. 3 for more.", "Next."],
            ),
            ("Item no. 3 is out.", &["Item no. 3 is out."]),
            ("Say no. Then go.", &["Say no.", "Then go."]),
            ("In 2024. Then 2025.", &["In 2024.", "Then 2025."]),
            ("The ratio is 3.5. Then.", &["The ratio is 3.5.", "Then."]),
            ("Edit main.rs. Then.", &["Edit main.rs.", "Then."]),
            ("A Ph.D. Student.", &["A Ph.D. Student."]),
            ("I agree. 3 do not.", &["I agree.", "3 do not."]),
            // The price of no language model: a sentence that ends in an
            // initial runs on.
            ("Use plan B. Then rest.", &["Use plan B. Then rest."]),
            // A placeholder (inline code) starts a sentence like a capital.
            ("Run it. \u{1A} prints.", &["Run it.", "\u{1A} prints."]),
            ("Ends with é. Ça va.", &["Ends with é.", "Ça va."]),
        ];
        for (text, expected) in cases {
            assert_eq!(split(text), *expected, "{text:?}");
        }
    }

    fn split_words(text: &str) -> Vec<&str> {
        words(text).map(|r| &text[r]).collect()
    }

    #[test]
    fn words_keep_inner_apostrophes() {
        assert_eq!(
            split_words("It's the dogs' 'toy'."),
            ["It's", "the", "dogs", "toy"]
        );
    }

    #[test]
    fn words_keep_inner_hyphens_and_digit_separators() {
        assert_eq!(
            split_words("A well-known, state-of-the-art tool - or -not- it."),
            [
                "A",
                "well-known",
                "state-of-the-art",
                "tool",
                "or",
                "not",
                "it"
            ]
        );
        assert_eq!(
            split_words("Up 3.5% to 1,000, then 2. Then a,b."),
            ["Up", "3.5", "to", "1,000", "then", "2", "Then", "a", "b"]
        );
    }

    #[test]
    fn syllable_counts() {
        let cases = [
            ("the", 1),
            ("cat", 1),
            ("ago", 2),
            ("are", 1),
            ("make", 1),
            ("whale", 1),
            ("table", 2),
            ("free", 1),
            ("syllable", 3),
            ("readability", 5),
            ("beautiful", 3),
            ("jumped", 1),
            ("wanted", 2),
            ("played", 1),
            ("makes", 1),
            ("boxes", 2),
            ("watches", 2),
            ("pages", 2),
            ("goes", 1),
            ("yes", 1),
            ("gym", 1),
            ("queue", 1),
            ("don't", 1),
            ("Café", 2),
            ("well-known", 2),
            ("2024", 1),
            ("_", 1),
        ];
        for (word, n) in cases {
            assert_eq!(syllables(word), n, "{word}");
        }
    }

    #[test]
    fn counts_add_up() {
        let a = Counts::of("The cat sat. A readability test!");
        assert_eq!(
            a,
            Counts {
                sentences: 2,
                words: 6,
                syllables: 10,
                polysyllables: 1,
                characters: 25,
            }
        );
        let mut b = Counts::of("One more.");
        b += a;
        assert_eq!((b.sentences, b.words), (3, 8));
        assert_eq!(Counts::of(""), Counts::default());
    }

    #[test]
    fn trimming() {
        assert_eq!(trimmed("  ab ", 0..5), 2..4);
        assert_eq!(trimmed("   ", 0..3), 3..3);
    }
}
