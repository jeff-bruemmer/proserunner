//! Inline control: comments that turn rules off and on inside a document.
//!
//! ```text
//! proserunner-disable [ids…]            off from here on
//! proserunner-enable [ids…]             on again from here
//! proserunner-disable-next-line [ids…]  off on the next line only
//! ```
//!
//! With no ids, a directive applies to every rule. Ids are separated by
//! spaces or commas, and `--` ends them, so the rest of the comment can
//! say why. Each scanner reads directives from its own comments:
//! `<!-- … -->` in Markdown and HTML, `{/* … */}` in MDX, `#` lines in
//! Org, `//` in AsciiDoc and Typst, and a line of its own in text.
//!
//! [`Control`] decides by where an issue starts, after matching, so a
//! directive works the same way for every kind of rule.

use std::ops::Range;

use super::piece::Piece;
use crate::typo;

/// Every directive starts with this.
const PREFIX: &str = "proserunner-";

const ACTIONS: [&str; 3] = ["disable", "enable", "disable-next-line"];

/// A word of a directive and the source offset it starts at.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Word {
    /// As written.
    pub text: String,
    /// Where it starts in the source.
    pub at: usize,
}

/// A directive as written. `action` is what follows `proserunner-`, which
/// may be a typo: [`Policy::apply`](crate::policy::Policy::apply) reports
/// it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Directive {
    /// `disable`, `enable`, `disable-next-line`, or a typo.
    pub action: Word,
    /// The rules it names; none for every rule.
    pub ids: Vec<Word>,
    /// The source offset just past the comment.
    pub end: usize,
}

/// A directive that doesn't parse or names a rule that doesn't exist.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
#[non_exhaustive]
#[must_use]
pub struct ControlError {
    /// The source offset of the word at fault.
    pub at: usize,
    /// What's wrong.
    pub message: String,
}

/// The directive in a comment's text, if it holds one. `offset` maps a
/// byte of `text` to its source offset; `end` is the source offset just
/// past the comment.
pub(super) fn parse(text: &str, end: usize, offset: impl Fn(usize) -> usize) -> Option<Directive> {
    let mut words = words(text).into_iter();
    let (at, first) = words.next()?;
    let action = first.strip_prefix(PREFIX)?;
    let ids: Vec<Word> = words
        .take_while(|&(_, w)| w != "--")
        .map(|(i, w)| Word {
            text: w.to_owned(),
            at: offset(i),
        })
        .collect();
    Some(Directive {
        action: Word {
            text: action.to_owned(),
            at: offset(at),
        },
        ids,
        end,
    })
}

/// Directives in the HTML comments of `pieces`, consecutive runs of
/// source text. A comment may span pieces, as the lines of an HTML block
/// do.
pub(super) fn html(pieces: &[Piece<'_>], out: &mut Vec<Directive>) {
    let mut text = String::new();
    // (offset in `text`, source offset) where each piece starts.
    let mut starts = Vec::with_capacity(pieces.len());
    for piece in pieces {
        starts.push((text.len(), piece.at));
        text.push_str(piece.text);
    }
    let source = |i: usize| {
        let p = starts.partition_point(|&(t, _)| t <= i).saturating_sub(1);
        starts.get(p).map_or(0, |&(t, s)| s + (i - t))
    };

    let mut from = 0;
    while let Some(open) = text[from..].find("<!--").map(|i| from + i) {
        let body = open + "<!--".len();
        let Some(close) = text[body..].find("-->").map(|i| body + i) else {
            break;
        };
        let end = close + "-->".len();
        if let Some(d) = parse(&text[body..close], source(end - 1) + 1, |i| {
            source(body + i)
        }) {
            out.push(d);
        }
        from = end;
    }
}

/// Words split on whitespace and commas, with their byte offsets.
fn words(text: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut start = None;
    for (i, c) in text.char_indices() {
        let separator = c.is_whitespace() || c == ',';
        match (separator, start) {
            (true, Some(s)) => {
                out.push((s, &text[s..i]));
                start = None;
            }
            (false, None) => start = Some(i),
            _ => {}
        }
    }
    if let Some(s) = start {
        out.push((s, &text[s..]));
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Disable,
    Enable,
    DisableNextLine,
}

fn action(word: &Word) -> Result<Action, ControlError> {
    match word.text.as_str() {
        "disable" => Ok(Action::Disable),
        "enable" => Ok(Action::Enable),
        "disable-next-line" => Ok(Action::DisableNextLine),
        other => {
            let hint = typo::closest(other, ACTIONS.into_iter())
                .map(|a| format!(" (did you mean `{PREFIX}{a}`?)"))
                .unwrap_or_default();
            Err(ControlError {
                at: word.at,
                message: format!("`{PREFIX}{other}` isn't a directive{hint}"),
            })
        }
    }
}

/// Which rules are off at a point, and the directive that turned each
/// off, by where it starts. Either every rule is on except those in
/// `toggled`, or (`all_off`) every rule is off except those in `toggled`.
#[derive(Debug, Clone)]
struct State<R> {
    all_off: Option<usize>,
    /// Sorted by rule; each with the directive that toggled it.
    toggled: Vec<(R, usize)>,
}

impl<R> Default for State<R> {
    fn default() -> Self {
        State {
            all_off: None,
            toggled: Vec::new(),
        }
    }
}

impl<R: Copy + Ord> State<R> {
    /// Where the directive that turned `rule` off starts, if it's off.
    fn off_by(&self, rule: R) -> Option<usize> {
        let toggled = self
            .toggled
            .binary_search_by_key(&rule, |&(r, _)| r)
            .ok()
            .map(|i| self.toggled[i].1);
        match self.all_off {
            Some(at) => toggled.is_none().then_some(at),
            None => toggled,
        }
    }

    /// Turns `rules` (`None`: every rule) off or on, by the directive that
    /// starts at `at`.
    fn set(&mut self, rules: Option<&[R]>, off: bool, at: usize) {
        let Some(rules) = rules else {
            self.all_off = off.then_some(at);
            self.toggled.clear();
            return;
        };
        let toggle = off != self.all_off.is_some();
        for &r in rules {
            match (self.toggled.binary_search_by_key(&r, |&(t, _)| t), toggle) {
                (Ok(i), false) => {
                    self.toggled.remove(i);
                }
                (Err(i), true) => self.toggled.insert(i, (r, at)),
                _ => {}
            }
        }
    }
}

/// Where a document's directives turn rules off. `R` names a rule, so
/// scanning needn't know about rules.
#[derive(Debug)]
pub(crate) struct Control<R> {
    /// From each offset on, until the next, the state that holds.
    changes: Vec<(usize, State<R>)>,
    /// Lines that `disable-next-line` covers, with its rules (`None`:
    /// every rule) and where the directive starts.
    lines: Vec<(Range<usize>, Option<Vec<R>>, usize)>,
}

impl<R: Copy + Ord> Control<R> {
    /// Resolves `directives` in `source`. `find` turns a rule id into a
    /// rule: `Ok(None)` for a rule from a source this run doesn't load,
    /// which a directive then leaves alone.
    pub(crate) fn try_new(
        directives: &[Directive],
        source: &str,
        mut find: impl FnMut(&str) -> Result<Option<R>, String>,
    ) -> Result<Self, ControlError> {
        let mut ordered: Vec<&Directive> = directives.iter().collect();
        ordered.sort_by_key(|d| d.end);

        let mut control = Control {
            changes: Vec::new(),
            lines: Vec::new(),
        };
        let mut state = State::default();
        for d in ordered {
            let action = action(&d.action)?;
            let mut rules = Vec::new();
            for id in &d.ids {
                let fail = |message: String| ControlError { at: id.at, message };
                rules.extend(find(&id.text).map_err(fail)?);
            }
            if !d.ids.is_empty() && rules.is_empty() {
                continue;
            }
            let named = (!d.ids.is_empty()).then_some(rules);
            match action {
                Action::Disable | Action::Enable => {
                    state.set(named.as_deref(), action == Action::Disable, d.action.at);
                    control.changes.push((d.end, state.clone()));
                }
                Action::DisableNextLine => {
                    control
                        .lines
                        .push((next_line(source, d.end), named, d.action.at));
                }
            }
        }
        Ok(control)
    }

    /// Whether `rule` is on at source offset `at`.
    pub(crate) fn allows(&self, rule: R, at: usize) -> bool {
        self.off_by(rule, at).is_none()
    }

    /// Where the directive that turns `rule` off at source offset `at`
    /// starts, or `None` when it's on there.
    pub(crate) fn off_by(&self, rule: R, at: usize) -> Option<usize> {
        let skipped_line = self.lines.iter().find_map(|(line, rules, by)| {
            (line.contains(&at) && rules.as_ref().is_none_or(|r| r.contains(&rule))).then_some(*by)
        });
        let i = self.changes.partition_point(|(from, _)| *from <= at);
        let off = i
            .checked_sub(1)
            .and_then(|i| self.changes.get(i))
            .and_then(|(_, state)| state.off_by(rule));
        skipped_line.or(off)
    }
}

/// The bytes of the line after the one holding offset `end`.
fn next_line(source: &str, end: usize) -> Range<usize> {
    let rest = source.get(end..).unwrap_or("");
    let Some(newline) = rest.find('\n') else {
        return source.len()..source.len();
    };
    let start = end + newline + 1;
    let len = source[start..].find('\n').unwrap_or(source.len() - start);
    start..start + len
}

#[cfg(test)]
mod tests {
    use super::*;

    fn directive(text: &str) -> Option<Directive> {
        parse(text, text.len(), |i| i)
    }

    #[test]
    fn parses_actions_ids_and_notes() {
        let d = directive(" proserunner-disable a/b, c/d -- quoting the spec ").unwrap();
        assert_eq!(d.action.text, "disable");
        assert_eq!(d.action.at, 1);
        let ids: Vec<_> = d.ids.iter().map(|w| (w.text.as_str(), w.at)).collect();
        assert_eq!(ids, [("a/b", 21), ("c/d", 26)]);
        assert!(directive("just a comment").is_none());
        assert!(directive("").is_none());
    }

    #[test]
    fn html_comments_across_pieces() {
        let src = "<!--\nproserunner-disable a/b\n--> <!-- other --> <!-- proserunner-enable -->";
        let pieces = [
            Piece {
                at: 0,
                text: &src[..5],
            },
            Piece {
                at: 5,
                text: &src[5..29],
            },
            Piece {
                at: 29,
                text: &src[29..],
            },
        ];
        let mut out = Vec::new();
        html(&pieces, &mut out);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].action.text, "disable");
        assert_eq!(&src[out[0].ids[0].at..out[0].ids[0].at + 3], "a/b");
        assert_eq!(out[0].end, 32);
        assert_eq!(out[1].action.text, "enable");
        assert_eq!(out[1].end, src.len());
    }

    fn resolve(src: &str, directives: &[Directive]) -> Result<Control<u32>, ControlError> {
        Control::try_new(directives, src, |id| match id {
            "t/a" => Ok(Some(0)),
            "t/b" => Ok(Some(1)),
            "other/x" => Ok(None),
            _ => Err(format!("no rule has the id `{id}`")),
        })
    }

    fn at(src: &str, text: &str, end_of: &str) -> Directive {
        let mut d = directive(text).unwrap();
        d.end = src.find(end_of).unwrap() + end_of.len();
        d
    }

    #[test]
    fn disable_and_enable_by_position() {
        let src = "one [D] two [E-a] three [E] four";
        let c = resolve(
            src,
            &[
                at(src, "proserunner-disable", "[D]"),
                at(src, "proserunner-enable t/a", "[E-a]"),
                at(src, "proserunner-enable", "[E]"),
            ],
        )
        .unwrap();
        let pos = |w: &str| src.find(w).unwrap();
        assert!(c.allows(0, pos("one")) && c.allows(1, pos("one")));
        assert!(!c.allows(0, pos("two")) && !c.allows(1, pos("two")));
        assert!(c.allows(0, pos("three")) && !c.allows(1, pos("three")));
        assert!(c.allows(0, pos("four")) && c.allows(1, pos("four")));
    }

    #[test]
    fn disable_next_line() {
        let src = "a\n[N]\nb b\nc";
        let c = resolve(src, &[at(src, "proserunner-disable-next-line t/b", "[N]")]).unwrap();
        let pos = |w: &str| src.find(w).unwrap();
        assert!(c.allows(1, pos("a")));
        assert!(!c.allows(1, pos("b")) && c.allows(0, pos("b")));
        assert!(c.allows(1, pos("c")));
    }

    #[test]
    fn unloaded_rules_change_nothing_and_typos_are_errors() {
        let src = "x";
        let c = resolve(src, &[at(src, "proserunner-disable other/x", "x")]).unwrap();
        assert!(c.allows(0, 1));

        let e = resolve(src, &[directive("proserunner-disabel").unwrap()]).unwrap_err();
        assert_eq!(
            e.message,
            "`proserunner-disabel` isn't a directive (did you mean `proserunner-disable`?)"
        );
        // The resolver judges every id, short ones too, and its error
        // points at the id.
        let e = resolve(src, &[directive("proserunner-disable hedging").unwrap()]).unwrap_err();
        assert_eq!(e.message, "no rule has the id `hedging`");
        assert_eq!(e.at, 20);
        let e = resolve(src, &[directive("proserunner-enable t/zz").unwrap()]).unwrap_err();
        assert_eq!(e.message, "no rule has the id `t/zz`");
    }

    #[test]
    fn off_by_names_the_directive_that_turned_a_rule_off() {
        let src = "[D] a [E-b] b [N]\nc\nd";
        let c = resolve(
            src,
            &[
                at(src, "proserunner-disable", "[D]"),
                at(src, "proserunner-enable t/b", "[E-b]"),
                at(src, "  proserunner-disable-next-line t/b", "[N]"),
            ],
        )
        .unwrap();
        // Each directive is parsed alone, so it starts where its text
        // does: 0, and 2 after the spaces.
        let pos = |w: &str| src.find(w).unwrap();
        assert_eq!(c.off_by(0, pos(" b ")), Some(0));
        assert_eq!(c.off_by(1, pos(" b ")), None);
        assert_eq!(c.off_by(1, pos("c")), Some(2));
        assert_eq!(c.off_by(1, pos("d")), None);
    }
}
