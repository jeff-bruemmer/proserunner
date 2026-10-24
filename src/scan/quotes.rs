//! Masks quoted text, so dialogue and citations aren't checked. Like a
//! URL, a quotation becomes placeholders: no phrase matches across it, and
//! it reads as one hidden thing.
//!
//! Curly double quotes always open a quote. A straight double quote opens
//! one only at the start of a word (`said "this`) and closes it only at the
//! end of one (`this" and`), so the inch mark in `5" wide` hides nothing.
//! Single quotes only count with whitespace or punctuation outside them,
//! so apostrophes (`it's`, `dogs'`) aren't mistaken for quotes.

use std::ops::Range;

use super::{Block, mask};

pub(super) fn mask(block: &mut Block) {
    let mut ranges = Vec::new();
    collect(&block.text, &mut ranges);
    mask::hide(block, ranges);
}

fn collect(text: &str, out: &mut Vec<Range<usize>>) {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let end_of = |i: usize| chars.get(i + 1).map_or(text.len(), |&(o, _)| o);
    let mut i = 0;

    let char_at = |i: Option<usize>| i.and_then(|i| chars.get(i)).map(|&(_, c)| c);
    let mut closers = Closers {
        chars: &chars,
        found: Vec::new(),
    };
    while i < chars.len() {
        let (start, c) = chars[i];
        let (before, after) = (char_at(i.checked_sub(1)), char_at(Some(i + 1)));
        let close = match c {
            '"' if opens(before, after) => Some('"'),
            '\u{201C}' => Some('\u{201D}'),
            '\'' | '\u{2018}' if delimits(i.checked_sub(1).map(|p| chars[p].1)) => {
                Some(if c == '\'' { '\'' } else { '\u{2019}' })
            }
            _ => None,
        };
        let single = matches!(c, '\'' | '\u{2018}');

        if let Some(close) = close
            && let Some(j) = closers.next(close, i + 1)
        {
            let after = chars.get(j + 1).map(|&(_, x)| x);
            if !single || delimits(after) {
                out.push(start..end_of(j));
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
}

/// Where each closing mark next is. Whether a char closes depends only on
/// where it is, so a search answers every later one that starts before
/// what it found, and one that found nothing answers all later ones: the
/// scan stays linear when quotes never close.
struct Closers<'text> {
    chars: &'text [(usize, char)],
    /// Per mark: where the last search started, and what it found.
    found: Vec<(char, usize, Option<usize>)>,
}

impl Closers<'_> {
    /// The index of the first `close` at or after `from` that closes.
    fn next(&mut self, close: char, from: usize) -> Option<usize> {
        let last = self.found.iter().position(|&(c, ..)| c == close);
        if let Some(&(_, start, found)) = last.map(|k| &self.found[k])
            && start <= from
            && found.is_none_or(|j| j >= from)
        {
            return found;
        }
        let chars = self.chars;
        let char_at = |j: Option<usize>| j.and_then(|j| chars.get(j)).map(|&(_, c)| c);
        let found = (from..chars.len()).find(|&j| {
            chars[j].1 == close
                && (close != '"' || closes(char_at(j.checked_sub(1)), char_at(Some(j + 1))))
        });
        match last {
            Some(k) => self.found[k] = (close, from, found),
            None => self.found.push((close, from, found)),
        }
        found
    }
}

/// Whether a straight double quote between `before` and `after` can open a
/// quote: at the start of a word, after a space or an opening bracket.
fn opens(before: Option<char>, after: Option<char>) -> bool {
    // U+2014 em dash, U+2013 en dash.
    let start = before.is_none_or(|c| {
        c.is_whitespace() || matches!(c, '(' | '[' | '{' | '\u{2014}' | '\u{2013}')
    });
    start && after.is_some_and(|c| !c.is_whitespace())
}

/// Whether a straight double quote between `before` and `after` can close
/// a quote: at the end of a word.
fn closes(before: Option<char>, after: Option<char>) -> bool {
    let end = after.is_none_or(|c| {
        c.is_whitespace()
            || matches!(c, ',' | ';' | ':' | '.' | '!' | '?' | ')' | ']' | '}' | '\'')
            // U+2014 em dash, U+2013 en dash, U+2019 right single quote.
            || matches!(c, '\u{2014}' | '\u{2013}' | '\u{2019}')
    });
    end && before.is_some_and(|c| !c.is_whitespace())
}

/// Whether `c`, the char outside a single quote, lets it be a quote.
fn delimits(c: Option<char>) -> bool {
    match c {
        None => true,
        Some(c) => c.is_whitespace() || matches!(c, ',' | ';' | ':' | '.' | '!' | '?' | '(' | ')'),
    }
}

#[cfg(test)]
mod tests {
    use super::collect;

    fn quoted(text: &str) -> Vec<&str> {
        let mut out = Vec::new();
        collect(text, &mut out);
        out.into_iter().map(|r| &text[r]).collect()
    }

    #[test]
    fn double_quotes() {
        assert_eq!(
            quoted(r#"He said "very unique" twice."#),
            [r#""very unique""#]
        );
        assert_eq!(quoted("He said “very unique” twice."), ["“very unique”"]);
    }

    #[test]
    fn single_quotes_need_delimiters() {
        assert_eq!(quoted("It's the dogs' bowl."), Vec::<&str>::new());
        assert_eq!(
            quoted("She said 'hello there' and 'bye'."),
            ["'hello there'", "'bye'"]
        );
        assert_eq!(quoted("A ‘curly’ one."), ["‘curly’"]);
    }

    #[test]
    fn unclosed_quotes_mask_nothing() {
        assert_eq!(quoted(r#"A 5" screen."#), Vec::<&str>::new());
    }

    #[test]
    fn straight_quotes_open_and_close_at_word_edges() {
        assert_eq!(
            quoted(r#"The board is 5" wide, perhaps. Then "quoted"."#),
            [r#""quoted""#]
        );
        assert_eq!(quoted(r#"A 5" and a 6" screen."#), Vec::<&str>::new());
        assert_eq!(
            quoted(r#""Hello," she said (" quietly ")."#),
            [r#""Hello,""#]
        );
        assert_eq!(quoted(r#"("a" or "b")"#), [r#""a""#, r#""b""#]);
    }
}
