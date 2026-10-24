//! `explain`: one rule, everything about it. What it is for, how each
//! entry matches as compiled, its examples, where its level was set, and
//! every setting that keeps its matches from being reported. Pure.

use std::fmt::Write as _;

use crate::config::Config;
use crate::ignores::IgnoreFile;
use crate::rules::{CompiledEntry, Engine, EntryKind, Exception, Fix, Form, Rule, Wording};
use crate::scan::Scope;
use crate::settings;

/// A suppression whose text is wider than this pushes only its own source
/// to the right.
const MAX_WIDTH: usize = 40;

/// An ignore list that applies here: how its path is shown, and what it
/// holds.
#[derive(Debug)]
#[non_exhaustive]
pub struct List<'list> {
    /// How its path is shown.
    pub path: &'list str,
    /// What it holds.
    pub file: &'list IgnoreFile,
}

impl<'list> List<'list> {
    /// The ignore list read from `path`.
    pub fn new(path: &'list str, file: &'list IgnoreFile) -> Self {
        List { path, file }
    }
}

/// Everything about `rule`, as `proserunner explain` prints it. `lists`
/// are the ignore lists that apply.
#[must_use]
pub fn render(rule: &Rule, config: &Config, lists: &[List<'_>]) -> String {
    use unicode_width::UnicodeWidthStr as _;

    let mut out = String::new();
    let _ = writeln!(out, "{}: {}", rule.id, rule.name);
    out.push('\n');

    // A built-in rule's origin is its id; any other's is its file.
    let builtin = rule.origin == rule.id;
    let settings = config.rule.get(&rule.id);
    let own_levels = rule.entries.iter().any(|e| e.level != rule.level);
    let level = match settings.and_then(|s| s.level.as_ref()) {
        Some(set) if own_levels => {
            format!("{} (set in {}, for every entry)", set.value, set.origin)
        }
        Some(set) => format!("{} (set in {})", set.value, set.origin),
        None => format!("{} (the rule's own)", rule.level),
    };
    let scope = scope(rule);
    let common = common_message(rule);
    let mut fields = vec![
        ("Level", level),
        (
            "Source",
            if builtin {
                "built in".to_owned()
            } else {
                rule.origin.clone()
            },
        ),
        ("Case", rule.case.to_string()),
        ("Scope", scope),
    ];
    if let Some(m) = common {
        fields.push(("Message", m.to_owned()));
    }
    for (name, value) in fields {
        let _ = writeln!(out, "{name:<8} {value}");
    }

    if let Some(explain) = rule.explain.as_deref().map(str::trim) {
        out.push('\n');
        for line in explain.lines() {
            let _ = writeln!(out, "  {line}");
        }
    }

    out.push_str("\nEntries\n");
    for (n, entry) in rule.entries.iter().enumerate() {
        let _ = writeln!(out, "  {}", heading(entry));
        match rule.forms(n) {
            Ok(forms) => {
                for form in forms {
                    let _ = writeln!(out, "      {}", describe(&form, &entry.source));
                }
            }
            Err(e) => {
                let _ = writeln!(out, "      doesn't compile: {e}");
            }
        }
        if entry.kind != EntryKind::Term && entry.case != rule.case {
            let _ = writeln!(out, "      case: {}", entry.case);
        }
        if entry.level != rule.level {
            let _ = writeln!(out, "      level: {}", entry.level);
        }
        message_line(&mut out, &entry.wording, common);
    }
    for c in &rule.checks {
        let description = c.check.describe();
        let _ = writeln!(out, "  {}", description.written);
        for line in description.finds.lines() {
            let _ = writeln!(out, "      {line}");
        }
        message_line(&mut out, &c.wording, common);
    }

    let ex = &rule.examples;
    if !(ex.bad.is_empty() && ex.good.is_empty() && ex.fix.is_empty()) {
        out.push_str("\nExamples\n");
        for bad in &ex.bad {
            let _ = writeln!(out, "  bad   {}", settings::quote(bad));
        }
        for good in &ex.good {
            let _ = writeln!(out, "  good  {}", settings::quote(good));
        }
        for Fix { before, after } in &ex.fix {
            let _ = writeln!(
                out,
                "  fix   {} → {}",
                settings::quote(before),
                settings::quote(after)
            );
        }
    }

    let unreported = suppressions(rule, config, lists);
    if !unreported.is_empty() {
        out.push_str("\nNever reported\n");
        let width = unreported
            .iter()
            .map(|(what, _)| what.width())
            .filter(|&w| w <= MAX_WIDTH)
            .max()
            .unwrap_or(0);
        for (what, from) in &unreported {
            let pad = " ".repeat(width.saturating_sub(what.width()));
            let _ = writeln!(out, "  {what}{pad}  {from}");
        }
    }

    let overrides = overrides(rule, config);
    if !overrides.is_empty() {
        out.push_str("\nOverrides\n");
        for line in overrides {
            let _ = writeln!(out, "  {line}");
        }
    }

    let _ = write!(
        out,
        "\nChange it in config\n  [rule.\"{}\"]\n  level = \"off\"  # or \"hint\", \"warning\" or \"error\"\n",
        rule.id
    );
    out
}

/// Where the rule applies, with the heading levels it keeps to.
fn scope(rule: &Rule) -> String {
    let levels = (!rule.heading_levels.is_empty()).then(|| {
        let levels: Vec<String> = rule.heading_levels.iter().map(u8::to_string).collect();
        let s = if levels.len() == 1 { "" } else { "s" };
        format!("level{s} {}", levels.join(", "))
    });
    let names: Vec<String> = rule
        .scope
        .iter()
        .map(|s| match (s, &levels) {
            (Scope::Heading, Some(levels)) => format!("heading ({levels})"),
            _ => s.to_string(),
        })
        .collect();
    match levels {
        _ if !names.is_empty() => names.join(", "),
        Some(levels) => format!("all prose (headings at {levels})"),
        None => "all prose".to_owned(),
    }
}

/// An entry as written: a word, a pattern or a term, and its suggestions
/// or a term's other spellings.
fn heading(entry: &CompiledEntry) -> String {
    let written = settings::quote(&entry.source);
    let quoted = |items: Vec<String>| items.join(", ");
    match entry.kind {
        EntryKind::Term if entry.variants.is_empty() => format!("term {written}"),
        EntryKind::Term => {
            let also = quoted(entry.variants.iter().map(|v| settings::quote(v)).collect());
            format!("term {written}, also {also}")
        }
        EntryKind::Word | EntryKind::Pattern => {
            let kind = if entry.kind == EntryKind::Pattern {
                "pattern "
            } else {
                ""
            };
            let suggest = &entry.wording.suggest;
            if suggest.is_empty() {
                format!("{kind}{written}")
            } else {
                let to = quoted(
                    suggest
                        .iter()
                        .map(|s| settings::quote(s.source()))
                        .collect(),
                );
                format!("{kind}{written} → {to}")
            }
        }
    }
}

/// The message most of the rule's entries and checks use, shown once at
/// the top. Ties go to the one written first. When each has a message of
/// its own, none is common.
fn common_message(rule: &Rule) -> Option<&str> {
    let mut counts: Vec<(&str, usize)> = Vec::new();
    let messages = rule
        .entries
        .iter()
        .map(|e| &e.wording)
        .chain(rule.checks.iter().map(|c| &c.wording))
        .map(|w| w.message.source());
    for m in messages {
        match counts.iter_mut().find(|(seen, _)| *seen == m) {
            Some((_, n)) => *n += 1,
            None => counts.push((m, 1)),
        }
    }
    let best = counts.iter().map(|(_, n)| *n).max()?;
    if best == 1 && counts.len() > 1 {
        return None;
    }
    counts.into_iter().find(|(_, n)| *n == best).map(|(m, _)| m)
}

/// An entry's or check's message, when it isn't the rule's common one.
fn message_line(out: &mut String, wording: &Wording, common: Option<&str>) {
    let message = wording.message.source();
    if common != Some(message) {
        let _ = writeln!(out, "      message: {message}");
    }
}

/// How a form matches. Its text is shown only when compiling changed the
/// entry: braces expanded, whitespace collapsed, or `(?i)` added.
fn describe(form: &Form, written: &str) -> String {
    let engine = match form.engine {
        Engine::Literal => "literal",
        Engine::Regex => "regex",
        Engine::Backtracking => "backtracking regex",
    };
    let edges = match (form.left, form.right) {
        (true, true) => ", whole word",
        (true, false) => ", starts a word",
        (false, true) => ", ends a word",
        (false, false) => "",
    };
    let limit = if form.engine == Engine::Backtracking {
        ", with a step limit"
    } else {
        ""
    };
    if form.text == written {
        format!("{engine}{edges}{limit}")
    } else {
        format!("{engine} {}{edges}{limit}", settings::quote(&form.text))
    }
}

/// What keeps a match from being reported, and where each came from.
fn suppressions(rule: &Rule, config: &Config, lists: &[List<'_>]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for e in &rule.except {
        let what = match e {
            Exception::Text(t) => settings::quote(t),
            Exception::Pattern { source, .. } => format!("pattern {}", settings::quote(source)),
        };
        out.push((what, "except, in the rule".to_owned()));
    }
    if let Some(settings) = config.rule.get(&rule.id) {
        for a in &settings.accept {
            out.push((
                settings::quote(&a.value),
                format!("accept for this rule, {}", a.origin),
            ));
        }
    }
    for a in &config.accept {
        out.push((settings::quote(&a.value), format!("accept, {}", a.origin)));
    }
    for list in lists {
        for a in &list.file.accept {
            out.push((settings::quote(a), format!("ignore list, {}", list.path)));
        }
        for e in &list.file.issue {
            if e.rule.as_deref().is_some_and(|r| r != rule.id) {
                continue;
            }
            out.push((
                format!("{} in {}", settings::quote(&e.matched), e.place()),
                format!("ignored issue, {}", list.path),
            ));
        }
    }
    out
}

/// `[[overrides]]` that change this rule for some paths.
fn overrides(rule: &Rule, config: &Config) -> Vec<String> {
    let mut out = Vec::new();
    for o in &config.overrides {
        let paths: Vec<String> = o
            .paths
            .written()
            .iter()
            .map(|p| settings::quote(&p.value))
            .collect();
        let paths = paths.join(", ");
        let mut changes = Vec::new();
        if let Some(settings) = o.rule.get(&rule.id) {
            if let Some(level) = &settings.level {
                changes.push(format!("level {}", level.value));
            }
            for a in &settings.accept {
                changes.push(format!(
                    "accept {} for this rule",
                    settings::quote(&a.value)
                ));
            }
        }
        for a in &o.accept {
            changes.push(format!("accept {}", settings::quote(a)));
        }
        if !changes.is_empty() {
            out.push(format!("{paths}: {} ({})", changes.join(", "), o.origin));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::*;
    use crate::config::{self, Layer, LayerKind};
    use crate::ctx::Ctx;
    use crate::ignores;
    use crate::rules::{RuleSet, RuleSource};

    fn rule(text: &str) -> RuleSet {
        let compiled = RuleSet::compile([RuleSource {
            id: "project/style".into(),
            origin: ".proserunner/rules/style.toml".into(),
            text: Cow::Owned(text.into()),
        }]);
        assert!(compiled.errors.is_empty(), "{:?}", compiled.errors);
        compiled.rules
    }

    fn config(text: &str) -> Config {
        let ctx = Ctx::new("/p".into(), Some("/home/me".into()), None, false);
        let layer = Layer {
            kind: LayerKind::Project,
            base: "/p".into(),
            within: Some("/p".into()),
            origin: ".proserunner/config.toml".into(),
            file: config::parse(text).unwrap(),
        };
        config::merge(&ctx, vec![layer]).unwrap()
    }

    #[test]
    fn explains_everything_about_a_rule() {
        let set = rule(
            r#"schema = 1
            message = "Prefer “{suggestion}”."
            explain = "House style."
            except = ["Leverage Partners"]
            words = [
              { match = "synerg{y,ies}", suggest = ["teamwork"] },
              { match = "über", suggest = ["over"] },
            ]
            patterns = [{ match = 'fast(?= track)', message = "No." }]
            [examples]
            bad = ["Synergy."]
            good = ["Teamwork."]
            "#,
        );
        let config = config(
            r#"schema = 1
            accept = ["Kubernetes"]
            [rule."project/style"]
            level = "error"
            accept = ["synergies"]
            [[overrides]]
            paths = ["legal/"]
            rule."project/style".level = "off"
            "#,
        );
        let list = ignores::parse(
            r#"schema = 1
            accept = ["hopefully"]
            [[issue]]
            file = "a.md"
            rule = "project/style"
            match = "synergy"
            line = 3
            [[issue]]
            file = "b.md"
            rule = "default/other"
            match = "x"
            "#,
        )
        .unwrap();
        let lists = [List::new(".proserunner/ignores.toml", &list)];
        let text = render(&set.rules()[0], &config, &lists);
        assert_eq!(
            text,
            r#"project/style: Style

Level    error (set in .proserunner/config.toml)
Source   .proserunner/rules/style.toml
Case     insensitive
Scope    all prose
Message  Prefer “{suggestion}”.

  House style.

Entries
  "synerg{y,ies}" → "teamwork"
      literal "synergy", whole word
      literal "synergies", whole word
  "über" → "over"
      regex "(?i)über", whole word
  pattern "fast(?= track)"
      backtracking regex "(?i)fast(?= track)", with a step limit
      message: No.

Examples
  bad   "Synergy."
  good  "Teamwork."

Never reported
  "leverage partners"  except, in the rule
  "synergies"          accept for this rule, .proserunner/config.toml
  "Kubernetes"         accept, .proserunner/config.toml
  "hopefully"          ignore list, .proserunner/ignores.toml
  "synergy" in a.md:3  ignored issue, .proserunner/ignores.toml

Overrides
  "legal/": level off (.proserunner/config.toml)

Change it in config
  [rule."project/style"]
  level = "off"  # or "hint", "warning" or "error"
"#
        );
    }

    #[test]
    fn explains_checks() {
        let set = rule(
            r#"schema = 1
            scope = ["paragraph", "heading"]
            heading-levels = [1, 2]
            consistent = [["advisor{,s}", "adviser{,s}"]]
            limit = { max = 30 }
            capitalization = { style = "title", except = ["Visual Studio Code"] }
            acronyms = { except = ["HTML"] }
            metric = { max = 10, min-words = 50 }
            "#,
        );
        let text = render(&set.rules()[0], &config("schema = 1"), &[]);
        assert_eq!(
            text,
            r#"project/style: Style

Level    warning (the rule's own)
Source   .proserunner/rules/style.toml
Case     insensitive
Scope    paragraph, heading (levels 1, 2)

Entries
  consistent
      "advisor{,s}" or "adviser{,s}"
      matched in any case; the first a document uses wins
      message: Prefer “{suggestion}”, which this document uses first.
  limit = { max = 30, unit = "words", per = "sentence" }
      a sentence of more than 30 words
      message: {count} {unit}; keep it to {limit} or fewer.
  capitalization = { style = "title" }
      words not in title case, leaving "Visual Studio Code" as written
      message: Use {style} case: “{suggestion}”.
  acronyms
      acronym: regex '\b([A-Z]{3,})s?\b'
      defined by: regex '\w \(([^()\s]+?)s?\)'
      except "HTML"
      an acronym's first use, unless it's defined by then
      message: Define “{acronym}” where it's first used, as in “Full Name ({acronym})”.
  metric = { formula = "flesch-kincaid", max = 10, per = "document", min-words = 50 }
      a document of 50 words or more above grade 10 by Flesch-Kincaid
      message: {formula} grade level {score}; aim for {limit} or lower.

Change it in config
  [rule."project/style"]
  level = "off"  # or "hint", "warning" or "error"
"#
        );
        let set = rule("schema = 1\nheading-levels = [0]\nlimit = { max = 8, per = \"block\" }");
        let text = render(&set.rules()[0], &config("schema = 1"), &[]);
        assert!(
            text.contains("Scope    all prose (headings at level 0)\nMessage  {count}"),
            "{text}"
        );
    }

    #[test]
    fn explains_terms_and_entry_settings() {
        let set = rule(
            r#"schema = 1
            message = "Write “{suggestion}”."
            except = [{ pattern = 'mac ?os 9' }]
            terms = ["GitHub", { term = "macOS", match = ["mac os{, x}"], level = "error" }]
            patterns = [{ match = '(\d+)%', suggest = ["{1} percent"], case = "sensitive" }]
            "#,
        );
        let config = config("schema = 1\n[rule.\"project/style\"]\nlevel = \"hint\"");
        let text = render(&set.rules()[0], &config, &[]);
        assert_eq!(
            text,
            r#"project/style: Style

Level    hint (set in .proserunner/config.toml, for every entry)
Source   .proserunner/rules/style.toml
Case     insensitive
Scope    all prose
Message  Write “{suggestion}”.

Entries
  pattern '(\d+)%' → "{1} percent"
      regex
      case: sensitive
  term "GitHub"
      literal, whole word
  term "macOS", also "mac os{, x}"
      literal, whole word
      literal "mac os", whole word
      literal "mac os x", whole word
      level: error

Never reported
  pattern "mac ?os 9"  except, in the rule

Change it in config
  [rule."project/style"]
  level = "off"  # or "hint", "warning" or "error"
"#
        );
    }
}
