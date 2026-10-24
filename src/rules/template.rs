//! Messages and suggestions with named placeholders. Every message can use
//! `{match}`, `{suggestion}` and `{rule}`; a check adds variables of its
//! own, and a pattern its groups (`{1}`, `{name}`). `{{` and `}}` are
//! literal braces. Each placeholder resolves to a slot when the rule
//! loads, so an unknown one is an error then, never a blank in the output.

use std::borrow::Cow;
use std::mem;

use crate::typo;

/// The variables every message can use, in slot order. A check's own
/// variables take the slots after them.
pub(crate) const COMMON: [&str; 3] = ["match", "suggestion", "rule"];

/// The slot of `{suggestion}` in [`COMMON`].
const SUGGESTION: usize = 1;

#[derive(Debug, Clone, PartialEq)]
enum Piece {
    Text(String),
    /// A variable: its slot in the values `render` takes.
    Var(usize),
}

/// A message or suggestion, its placeholders resolved to slots.
#[derive(Debug, Clone, PartialEq)]
pub struct Template {
    /// Empty when the text has no braces, so it renders as written.
    pieces: Vec<Piece>,
    /// The text as written.
    source: String,
}

impl Template {
    /// Parses a message whose placeholders are the common ones (`{match}`,
    /// `{suggestion}`, `{rule}`) and then `vars`.
    pub fn parse(s: &str, vars: &[&str]) -> Result<Template, String> {
        Self::parse_in(s, vars, "message")
    }

    /// Parses a suggestion, which can use a message's placeholders except
    /// `{suggestion}`.
    pub fn suggestion(s: &str, vars: &[&str]) -> Result<Template, String> {
        let t = Self::parse_in(s, vars, "suggestion")?;
        if t.uses_suggestion() {
            return Err(format!(
                "`{s}`: a suggestion can't use {{suggestion}}, only {{match}}, {{rule}} and a pattern's groups"
            ));
        }
        Ok(t)
    }

    /// Text with no placeholders, used as it is.
    pub fn literal(text: &str) -> Template {
        if !text.contains(['{', '}']) {
            return Template::plain(text);
        }
        Template {
            pieces: vec![Piece::Text(text.to_owned())],
            source: text.replace('{', "{{").replace('}', "}}"),
        }
    }

    /// Text without braces: most suggestions, so they cost one string.
    fn plain(text: &str) -> Template {
        Template {
            pieces: Vec::new(),
            source: text.to_owned(),
        }
    }

    /// `what` names the text in errors: "message" or "suggestion".
    fn parse_in(s: &str, vars: &[&str], what: &str) -> Result<Template, String> {
        if !s.contains(['{', '}']) {
            return Ok(Template::plain(s));
        }
        let names: Vec<&str> = COMMON.iter().chain(vars).copied().collect();
        let mut pieces = Vec::new();
        let mut text = String::new();
        let mut chars = s.chars().peekable();

        while let Some(c) = chars.next() {
            match c {
                '{' if chars.peek() == Some(&'{') => {
                    chars.next();
                    text.push('{');
                }
                '}' if chars.peek() == Some(&'}') => {
                    chars.next();
                    text.push('}');
                }
                '{' => {
                    let mut name = String::new();
                    let mut closed = false;
                    for c in chars.by_ref() {
                        if c == '}' {
                            closed = true;
                            break;
                        }
                        name.push(c);
                    }
                    if !closed {
                        return Err(format!(
                            "unclosed `{{{name}` in {what} (write {{{{ for a literal brace)"
                        ));
                    }
                    let slot = names
                        .iter()
                        .position(|n| *n == name)
                        .ok_or_else(|| unknown(&name, &names, what))?;
                    if !text.is_empty() {
                        pieces.push(Piece::Text(mem::take(&mut text)));
                    }
                    pieces.push(Piece::Var(slot));
                }
                '}' => {
                    return Err(format!(
                        "unmatched `}}` in {what} (write }}}} for a literal brace)"
                    ));
                }
                c => text.push(c),
            }
        }
        if !text.is_empty() {
            pieces.push(Piece::Text(text));
        }
        Ok(Template {
            pieces,
            source: s.to_owned(),
        })
    }

    /// The message as written, braces escaped.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Whether it uses `{suggestion}`.
    pub fn uses_suggestion(&self) -> bool {
        self.pieces.contains(&Piece::Var(SUGGESTION))
    }

    /// Whether it uses a variable beyond `{match}`, `{suggestion}` and
    /// `{rule}`, such as a group.
    pub fn uses_vars(&self) -> bool {
        self.pieces
            .iter()
            .any(|p| matches!(p, Piece::Var(slot) if *slot >= COMMON.len()))
    }

    /// The message with each variable replaced by `values[slot]`. Text
    /// without variables is borrowed.
    pub fn render(&self, values: &[&str]) -> Cow<'_, str> {
        let pieces = match self.pieces.as_slice() {
            [] => return Cow::Borrowed(&self.source),
            [Piece::Text(t)] => return Cow::Borrowed(t),
            pieces => pieces,
        };
        let mut out = String::new();
        for piece in pieces {
            match piece {
                Piece::Text(t) => out.push_str(t),
                Piece::Var(slot) => out.push_str(values.get(*slot).copied().unwrap_or_default()),
            }
        }
        Cow::Owned(out)
    }
}

/// Why `{name}` isn't a placeholder: which groups there are, for a
/// number; else the closest one it may mean, or all of them.
fn unknown(name: &str, names: &[&str], what: &str) -> String {
    if let Ok(n) = name.parse::<usize>() {
        let groups = names.iter().filter(|n| n.parse::<usize>().is_ok()).count();
        let why = match (n, groups) {
            (0, _) => "use {match} for the whole match".to_owned(),
            (_, 0) => "only a pattern's groups are numbered, and this has none".to_owned(),
            (_, 1) => "the pattern has one group, {1}".to_owned(),
            (_, g) => format!("the pattern has groups {{1}} to {{{g}}}"),
        };
        return format!("unknown placeholder `{{{name}}}` in {what} ({why})");
    }
    if let Some(near) = typo::closest(name, names.iter().copied()) {
        return format!("unknown placeholder `{{{name}}}` in {what} (did you mean `{{{near}}}`?)");
    }
    let braced: Vec<String> = names.iter().map(|n| format!("{{{n}}}")).collect();
    let list = match braced.split_last() {
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} or {last}", rest.join(", ")),
        None => String::new(),
    };
    format!(
        "unknown placeholder `{{{name}}}` in {what} (use {list}; write {{{{ for a literal brace)"
    )
}

#[cfg(test)]
mod tests {
    use super::Template;

    #[test]
    fn renders_placeholders() {
        let t = Template::parse("Prefer “{suggestion}” to “{match}” ({rule}).", &[]).unwrap();
        assert_eq!(
            t.render(&["in order to", "to", "Wordiness"]),
            "Prefer “to” to “in order to” (Wordiness)."
        );
    }

    #[test]
    fn a_checks_variables_follow_the_common_ones() {
        let t = Template::parse(
            "{count} {unit}, over {limit}: {match}",
            &["count", "limit", "unit"],
        )
        .unwrap();
        assert_eq!(
            t.render(&["It went on.", "", "Limit", "31", "30", "words"]),
            "31 words, over 30: It went on."
        );
        let e = Template::parse("{count}", &[]).unwrap_err();
        assert!(e.contains("`{count}`"), "{e}");
    }

    #[test]
    fn escaped_braces() {
        let t = Template::parse("{{literal}} {match}", &[]).unwrap();
        assert_eq!(t.render(&["m"]), "{literal} m");
    }

    #[test]
    fn source_round_trips() {
        let written = "Prefer “{suggestion}” to {{x}} ({rule}): {match}";
        assert_eq!(Template::parse(written, &[]).unwrap().source(), written);
    }

    #[test]
    fn uses_suggestion() {
        let uses = |s| Template::parse(s, &[]).unwrap().uses_suggestion();
        assert!(uses("Prefer {suggestion}."));
        assert!(!uses("Avoid {match} ({rule}), not {{suggestion}}."));
    }

    #[test]
    fn unknown_placeholders_name_the_closest() {
        let e = Template::parse("Avoid %s or {mtach}", &[]).unwrap_err();
        assert_eq!(
            e,
            "unknown placeholder `{mtach}` in message (did you mean `{match}`?)"
        );
        let e = Template::parse("{limt}", &["count", "limit"]).unwrap_err();
        assert!(e.contains("did you mean `{limit}`?"), "{e}");
    }

    #[test]
    fn unknown_placeholders_list_every_one_when_none_is_close() {
        let e = Template::parse("{x}", &["count"]).unwrap_err();
        assert_eq!(
            e,
            "unknown placeholder `{x}` in message \
             (use {match}, {suggestion}, {rule} or {count}; write {{ for a literal brace)"
        );
    }

    #[test]
    fn unknown_groups_say_which_there_are() {
        let e = |s, vars: &[&str]| Template::parse(s, vars).unwrap_err();
        assert_eq!(
            e("{3}", &["1", "2", "year"]),
            "unknown placeholder `{3}` in message (the pattern has groups {1} to {2})"
        );
        assert!(e("{2}", &["1"]).ends_with("(the pattern has one group, {1})"));
        assert!(e("{1}", &[]).ends_with("this has none)"));
        assert!(e("{0}", &["1"]).ends_with("(use {match} for the whole match)"));
    }

    #[test]
    fn groups_are_variables() {
        let t = Template::parse("{1} or {year}", &["1", "year"]).unwrap();
        assert!(t.uses_vars());
        assert_eq!(t.render(&["m", "", "r", "a", "b"]), "a or b");
        assert!(!Template::parse("{match}", &["1"]).unwrap().uses_vars());
    }

    #[test]
    fn suggestions_cant_use_suggestion() {
        let t = Template::suggestion("{1}s", &["1"]).unwrap();
        assert_eq!(t.render(&["m", "", "r", "cat"]), "cats");
        let e = Template::suggestion("{suggestion}", &[]).unwrap_err();
        assert!(e.contains("can't use {suggestion}"), "{e}");
        let e = Template::suggestion("{sugest}", &[]).unwrap_err();
        assert!(e.contains("in suggestion"), "{e}");
    }

    #[test]
    fn literals_keep_their_braces() {
        let t = Template::literal("a{b}");
        assert_eq!(t.render(&[]), "a{b}");
        assert_eq!(t.source(), "a{{b}}");
        assert_eq!(Template::parse(t.source(), &[]).unwrap(), t);
    }

    #[test]
    fn unclosed_placeholder_is_an_error() {
        let e = Template::parse("Prefer {match", &[]).unwrap_err();
        assert!(e.contains("unclosed `{match`"), "{e}");
    }

    #[test]
    fn unmatched_close_is_an_error() {
        let e = Template::parse("Prefer } this", &[]).unwrap_err();
        assert!(e.contains("unmatched `}`"), "{e}");
    }
}
