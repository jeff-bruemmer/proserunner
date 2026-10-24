//! Converts 0.8 check files (EDN) into rule files (TOML).
//!
//! In 0.8 every specimen was silently a regex fragment. Here a specimen
//! that is a plain literal stays a word; one whose regex matches a small
//! finite set of strings (`thought leaders?`) becomes those words; anything
//! else becomes a pattern with explicit `\b` boundaries.

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use regex_syntax::ParserBuilder;
use regex_syntax::hir::HirKind;
use regex_syntax::hir::literal::{ExtractKind, Extractor};

use super::edn::{self, Edn};
use crate::error::{Error, Result};
use crate::fsutil;
use crate::prose;
use crate::settings;

/// A regex that matches more strings than this stays a pattern.
const MAX_EXPANDED: usize = 32;

#[derive(Debug, Clone, PartialEq)]
enum Specimen {
    Words(Vec<String>),
    Pattern(String),
}

fn convert_specimen(s: &str) -> Specimen {
    let Ok(hir) = ParserBuilder::new().build().parse(s) else {
        return Specimen::Pattern(bounded(s));
    };
    if let HirKind::Literal(lit) = hir.kind()
        && let Ok(word) = str::from_utf8(&lit.0)
    {
        return Specimen::Words(vec![word.to_owned()]);
    }
    let mut extractor = Extractor::new();
    extractor
        .kind(ExtractKind::Prefix)
        .limit_total(MAX_EXPANDED);
    let seq = extractor.extract(&hir);
    if seq.is_exact()
        && let Some(literals) = seq.literals()
    {
        let words: Option<Vec<String>> = literals
            .iter()
            .map(|l| str::from_utf8(l.as_bytes()).ok().map(str::to_owned))
            .collect();
        if let Some(words) =
            words.filter(|w| !w.is_empty() && w.iter().all(|w| !w.trim().is_empty()))
        {
            return Specimen::Words(words);
        }
    }
    Specimen::Pattern(bounded(s))
}

fn bounded(s: &str) -> String {
    format!(r"\b(?:{s})\b")
}

/// Like Clojure's `string/capitalize`, which 0.8 applied to names.
fn display_name(name: &str) -> String {
    let spaced = name.replace('-', " ");
    let mut chars = spaced.chars();
    match chars.next() {
        Some(first) => first
            .to_uppercase()
            .chain(chars.flat_map(char::to_lowercase))
            .collect(),
        None => spaced,
    }
}

/// 0.8 text as a 1.0 message or suggestion: it had no placeholders, so
/// every brace is literal.
fn literal(text: &str) -> String {
    text.replace('{', "{{").replace('}', "}}")
}

/// What 0.8 did to messages before showing them: a capital first letter
/// and a final period.
fn sentence(message: &str) -> String {
    let mut chars = message.trim().chars();
    let mut s: String = match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => return String::new(),
    };
    if !s.ends_with(['.', '!', '?']) {
        s.push('.');
    }
    s
}

struct Rule {
    name: String,
    message: Option<String>,
    explain: Option<String>,
    sensitive: bool,
    words: Vec<WrittenEntry>,
    patterns: Vec<WrittenEntry>,
    repeat: bool,
}

/// A `words` or `patterns` entry to write.
struct WrittenEntry {
    /// What it matches.
    pattern: String,
    suggest: Vec<String>,
    message: Option<String>,
}

fn strings(edn: &Edn, key: &str) -> Vec<String> {
    edn.get(key)
        .and_then(Edn::items)
        .map(|items| {
            items
                .iter()
                .filter_map(Edn::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn text(edn: &Edn, key: &str) -> Option<String> {
    edn.get(key).and_then(Edn::as_str).map(str::to_owned)
}

impl TryFrom<&Edn> for Rule {
    type Error = String;

    fn try_from(check: &Edn) -> Result<Rule, String> {
        let kind = text(check, "kind").ok_or("the check has no :kind")?;
        let name = text(check, "name").ok_or("the check has no :name")?;
        let mut rule = Rule {
            name: display_name(&name),
            message: text(check, "message").map(|m| sentence(&m)),
            explain: text(check, "explanation"),
            sensitive: matches!(kind.as_str(), "case" | "case-recommender" | "regex"),
            words: Vec::new(),
            patterns: Vec::new(),
            repeat: false,
        };

        match kind.as_str() {
            "existence" | "case" => {
                for specimen in strings(check, "specimens") {
                    rule.add(&specimen, Vec::new(), None);
                }
            }
            "recommender" | "case-recommender" => {
                // 0.8 ignored a recommender's :message and showed "Prefer: …".
                rule.message = None;
                let recommendations = check
                    .get("recommendations")
                    .and_then(Edn::items)
                    .unwrap_or(&[]);
                for r in recommendations {
                    let (Some(avoid), Some(prefer)) = (text(r, "avoid"), text(r, "prefer")) else {
                        return Err("a recommendation needs :avoid and :prefer".into());
                    };
                    let suggest: Vec<String> = prefer
                        .split(',')
                        .map(|s| s.trim().to_owned())
                        .filter(|s| !s.is_empty())
                        .collect();
                    rule.add(&avoid, suggest, None);
                }
            }
            "regex" => {
                let expressions = check.get("expressions").and_then(Edn::items).unwrap_or(&[]);
                for e in expressions {
                    let re = text(e, "re").ok_or("an expression needs :re")?;
                    let message = text(e, "message").map(|m| sentence(&m));
                    rule.patterns.push(WrittenEntry {
                        pattern: re,
                        suggest: Vec::new(),
                        message,
                    });
                }
            }
            "repetition" => rule.repeat = true,
            other => return Err(format!("unknown :kind \"{other}\"")),
        }
        Ok(rule)
    }
}

impl Rule {
    /// Adds a specimen, merging suggestions for a word already present.
    fn add(&mut self, specimen: &str, suggest: Vec<String>, message: Option<String>) {
        match convert_specimen(specimen) {
            Specimen::Words(words) => {
                for word in words {
                    let word = prose::normalize(&word);
                    if let Some(existing) = self.words.iter_mut().find(|w| w.pattern == word) {
                        for s in &suggest {
                            if !existing.suggest.contains(s) {
                                existing.suggest.push(s.clone());
                            }
                        }
                    } else {
                        self.words.push(WrittenEntry {
                            pattern: word,
                            suggest: suggest.clone(),
                            message: message.clone(),
                        });
                    }
                }
            }
            Specimen::Pattern(p) => {
                if !self.patterns.iter().any(|q| q.pattern == p) {
                    self.patterns.push(WrittenEntry {
                        pattern: p,
                        suggest,
                        message,
                    });
                }
            }
        }
    }

    fn to_toml(&self) -> String {
        let mut out = String::from("schema = 1\n");
        let _ = writeln!(out, "name = {}", settings::quote(&self.name));
        if let Some(m) = &self.message {
            let _ = writeln!(out, "message = {}", settings::quote(&literal(m)));
        }
        if let Some(e) = &self.explain {
            let _ = writeln!(out, "explain = {}", settings::quote(e));
        }
        if self.sensitive {
            out.push_str("case = \"sensitive\"\n");
        }
        if self.repeat {
            out.push_str("repeat = true\n");
        }
        write_entries(&mut out, "words", &self.words);
        write_entries(&mut out, "patterns", &self.patterns);

        let mut bad = Vec::new();
        let mut fix = Vec::new();
        if let Some(WrittenEntry {
            pattern: word,
            suggest,
            ..
        }) = self.words.first()
        {
            bad.push(format!("Example: {word}."));
            if let Some(s) = suggest.first() {
                fix.push((format!("Example: {word}."), format!("Example: {s}.")));
            }
        }
        if self.repeat {
            bad.push("This is is a test.".to_owned());
        }
        if !bad.is_empty() {
            out.push_str("\n[examples]\n");
            let list: Vec<String> = bad.iter().map(|b| settings::quote(b)).collect();
            let _ = writeln!(out, "bad = [{}]", list.join(", "));
            if !fix.is_empty() {
                let pairs: Vec<String> = fix
                    .iter()
                    .map(|(a, b)| format!("[{}, {}]", settings::quote(a), settings::quote(b)))
                    .collect();
                let _ = writeln!(out, "fix = [{}]", pairs.join(", "));
            }
        }
        out
    }
}

fn write_entries(out: &mut String, key: &str, entries: &[WrittenEntry]) {
    if entries.is_empty() {
        return;
    }
    let _ = writeln!(out, "\n{key} = [");
    for entry in entries {
        let WrittenEntry {
            pattern: m,
            suggest,
            message,
        } = entry;
        if suggest.is_empty() && message.is_none() {
            let _ = writeln!(out, "    {},", settings::quote(m));
            continue;
        }
        let mut fields = vec![format!("match = {}", settings::quote(m))];
        if !suggest.is_empty() {
            let list: Vec<String> = suggest
                .iter()
                .map(|s| settings::quote(&literal(s)))
                .collect();
            fields.push(format!("suggest = [{}]", list.join(", ")));
        }
        if let Some(message) = message {
            fields.push(format!("message = {}", settings::quote(&literal(message))));
        }
        let _ = writeln!(out, "    {{ {} }},", fields.join(", "));
    }
    out.push_str("]\n");
}

/// Converts one 0.8 check file's text to rule TOML.
pub fn convert(check_edn: &str) -> Result<String, String> {
    let check = edn::parse(check_edn)?;
    Ok(Rule::try_from(&check)?.to_toml())
}

/// Converts every `*.edn` check directly in `src` into `dst/<stem>.toml`.
/// Refuses to overwrite. Returns the files written.
pub fn convert_dir(src: &Path, dst: &Path) -> Result<Vec<String>> {
    let paths = fsutil::files_with_extension(src, "edn")?;

    let mut converted = Vec::new();
    for path in &paths {
        let text = fs::read_to_string(path).map_err(|e| Error::io(path, e))?;
        let toml =
            convert(&text).map_err(|e| Error::Migrate(format!("{}: {e}", path.display())))?;
        converted.push((path, toml));
    }

    fs::create_dir_all(dst).map_err(|e| Error::io(dst, e))?;
    let mut written = Vec::new();
    for (path, toml) in converted {
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let out = dst.join(format!("{stem}.toml"));
        if out.exists() {
            return Err(Error::Migrate(format!(
                "{} already exists; not overwriting it",
                out.display()
            )));
        }
        fs::write(&out, toml).map_err(|e| Error::io(&out, e))?;
        written.push(out.display().to_string());
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::*;
    use crate::rules::{RuleSet, RuleSource};

    fn compiles(toml: &str) {
        let errors = RuleSet::compile([RuleSource {
            id: "t/x".into(),
            origin: "t/x".into(),
            text: Cow::Borrowed(toml),
        }])
        .errors;
        assert!(errors.is_empty(), "{errors:?}\n{toml}");
    }

    #[test]
    fn specimens_become_words_when_they_can() {
        assert_eq!(
            convert_specimen("I think"),
            Specimen::Words(vec!["I think".into()])
        );
        assert_eq!(
            convert_specimen("thought leaders?"),
            Specimen::Words(vec!["thought leaders".into(), "thought leader".into()])
        );
        assert_eq!(
            convert_specimen("whet (?:the|your) appetite"),
            Specimen::Words(vec![
                "whet the appetite".into(),
                "whet your appetite".into()
            ])
        );
        assert_eq!(
            convert_specimen("e.g."),
            Specimen::Pattern(r"\b(?:e.g.)\b".into())
        );
    }

    #[test]
    fn existence() {
        let toml = convert(r#"{:name "hedging" :kind "existence" :message "omit this" :explanation "Say it." :specimens ["I think" "perhaps"]}"#).unwrap();
        assert!(toml.contains("name = \"Hedging\""), "{toml}");
        assert!(toml.contains("message = \"Omit this.\""), "{toml}");
        assert!(toml.contains("bad = [\"Example: I think.\"]"), "{toml}");
        compiles(&toml);
    }

    #[test]
    fn recommender_merges_and_splits_suggestions() {
        let toml = convert(
            r#"{:name "compression" :kind "recommender"
                :recommendations [{:avoid "due to the fact that" :prefer "because, since"}
                                  {:avoid "due to the fact that" :prefer "because"}]}"#,
        )
        .unwrap();
        assert!(
            toml.contains(r#"{ match = "due to the fact that", suggest = ["because", "since"] },"#),
            "{toml}"
        );
        assert!(!toml.contains("message ="), "{toml}");
        compiles(&toml);
    }

    #[test]
    fn regex_and_repetition() {
        let toml = convert(r#"{:name "regex" :kind "regex" :message "m" :expressions [{:re "a\\.(?!.)" :message "no period"}]}"#).unwrap();
        assert!(
            toml.contains(r#"{ match = 'a\.(?!.)', message = "No period." }"#),
            "{toml}"
        );
        assert!(toml.contains("case = \"sensitive\""), "{toml}");
        compiles(&toml);

        let toml = convert(
            r#"{:name "repetition" :kind "repetition" :message "Consecutive word repetition"}"#,
        )
        .unwrap();
        assert!(toml.contains("repeat = true"), "{toml}");
        compiles(&toml);
    }

    #[test]
    fn braces_stay_literal() {
        let toml = convert(
            r#"{:name "braces" :kind "recommender"
                :recommendations [{:avoid "{{ x }}" :prefer "{x}"}]}"#,
        )
        .unwrap();
        assert!(toml.contains(r#"suggest = ["{{x}}"]"#), "{toml}");
        compiles(&toml);
        let toml =
            convert(r#"{:name "b" :kind "existence" :message "no {x}" :specimens ["a"]}"#).unwrap();
        assert!(toml.contains(r#"message = "No {{x}}.""#), "{toml}");
        compiles(&toml);
    }

    #[test]
    fn unknown_kinds_are_errors() {
        assert!(
            convert(r#"{:name "x" :kind "grammar"}"#)
                .unwrap_err()
                .contains("grammar")
        );
    }
}
