//! `repeat = true`: a word repeated right after itself, in any case
//! ("the the").

use super::{Check, Description, Found, Spec};
use crate::prose;
use crate::scan::Block;

pub(super) const SPEC: Spec = Spec {
    key: "repeat",
    vars: &[],
    message: "Repeated word: “{match}”.",
    suggest: &[],
};

pub(super) fn compile(on: bool) -> Option<Check> {
    on.then_some(Check::Repeat)
}

pub(super) fn describe() -> Description {
    Description {
        written: "repeat = true".into(),
        finds: "a word twice in a row, any case".into(),
    }
}

/// A word followed by the same word, ignoring case. The words must be
/// separated only by whitespace in the source, or by emphasis or quote
/// markers around them.
pub(super) fn block(block: &Block, source: &str, out: &mut Vec<Found>) {
    let mut words = prose::words(&block.text);
    let Some(mut a) = words.next() else {
        return;
    };
    for b in words {
        let gap = &block.text[a.end..b.start];
        let found = gap.chars().all(char::is_whitespace)
            && !gap.is_empty()
            && same_word(&block.text[a.clone()], &block.text[b.clone()])
            && source
                .get(block.source_offset(a.end - 1) + 1..block.source_offset(b.start))
                .unwrap_or("")
                .chars()
                .all(|c| c.is_whitespace() || matches!(c, '*' | '_' | '~' | '>'));
        if found {
            out.push(Found {
                range: a.start..b.end,
                args: Vec::new(),
            });
        }
        a = b;
    }
}

/// Whether two words are the same, ignoring case. Stops at the first
/// letter that differs, without allocating.
fn same_word(a: &str, b: &str) -> bool {
    a == b
        || a.chars()
            .flat_map(char::to_lowercase)
            .eq(b.chars().flat_map(char::to_lowercase))
}

#[cfg(test)]
mod tests {
    use crate::rules::{EntryRef, compile_one};
    use crate::scan::{self, Format, Options};

    /// What `block` finds in each block of Markdown `source`.
    fn repeated(source: &str) -> Vec<String> {
        let mut found = Vec::new();
        for b in scan::scan(Format::Markdown, source, Options::default()).blocks {
            let mut out = Vec::new();
            super::block(&b, source, &mut out);
            found.extend(out.into_iter().map(|f| b.text[f.range].to_owned()));
        }
        found
    }

    #[test]
    fn repeated_words() {
        assert_eq!(repeated("This is is a test."), ["is is"]);
        assert_eq!(repeated("The the end."), ["The the"]);
        assert_eq!(repeated("the\nthe end"), ["the the"]);
        assert_eq!(repeated("*the* the end"), ["the the"]);
        assert_eq!(repeated("> the\n> the end"), ["the the"]);
        assert!(repeated("the `x` the").is_empty());
        assert!(repeated("the, the").is_empty());
        assert!(repeated("It's its own.").is_empty());
        // Overlapping pairs: the rule keeps the first.
        assert_eq!(repeated("the the the"), ["the the", "the the"]);
    }

    #[test]
    fn default_message() {
        let set = compile_one("schema = 1\nrepeat = true").unwrap();
        let wording = set.rules[0].wording(EntryRef::Check(0)).unwrap();
        assert_eq!(
            wording.render("the the", "R", &[]),
            "Repeated word: “the the”."
        );
    }

    #[test]
    fn its_message_cant_use_suggestion() {
        let e =
            compile_one("schema = 1\nmessage = \"Use {suggestion}\"\nrepeat = true").unwrap_err();
        assert!(e.contains("`repeat` has no suggestions"), "{e}");
    }
}
