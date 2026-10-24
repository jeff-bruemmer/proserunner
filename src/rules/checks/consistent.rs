//! `consistent = [["advisor", "adviser"]]`: spellings that are each fine,
//! as long as a document keeps to one. The first a document uses wins,
//! and each other spelling in its group is reported.

use super::{Check, Description, Doc, Found, FoundIn, Spec, Want};
use crate::prose;
use crate::rules::expand;
use crate::rules::rule_file::Case;
use crate::settings;

pub(super) const SPEC: Spec = Spec {
    key: "consistent",
    vars: &["form"],
    message: "Prefer “{suggestion}”, which this document uses first.",
    suggest: &["{form}"],
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Consistent {
    /// Each group's forms, as written.
    groups: Vec<Vec<String>>,
    /// `spellings[g][f][k]`: form `f` of group `g`, brace expansion `k`.
    spellings: Vec<Vec<Vec<String>>>,
    /// What each want stands for: its group, form and expansion.
    tags: Vec<(usize, usize, usize)>,
    case: Case,
}

/// Every form in a group expands to as many spellings, so each spelling
/// has one in every other form to suggest: `advisor{,s}` and
/// `adviser{,s}`.
pub(super) fn compile(groups: &[Vec<String>], case: Case) -> Result<Option<Check>, String> {
    if groups.is_empty() {
        return Ok(None);
    }
    let mut spellings = Vec::new();
    let mut tags = Vec::new();
    for (g, forms) in groups.iter().enumerate() {
        if forms.len() < 2 {
            return Err(format!(
                "each group needs two forms or more, not [{}]",
                list(forms)
            ));
        }
        let expanded: Vec<Vec<String>> = forms
            .iter()
            .map(|form| spellings_of(form))
            .collect::<Result<_, _>>()?;
        if let Some(f) = (1..forms.len()).find(|&f| expanded[f].len() != expanded[0].len()) {
            return Err(format!(
                "`{}` and `{}` expand to {} and {} spellings; give each form the same braces, so each spelling has one to suggest",
                forms[0],
                forms[f],
                expanded[0].len(),
                expanded[f].len()
            ));
        }
        let fold = |s: &str| match case {
            Case::Insensitive => s.to_lowercase(),
            Case::Sensitive => s.to_owned(),
        };
        let mut seen: Vec<(String, &str)> = Vec::new();
        for (f, words) in expanded.iter().enumerate() {
            for (k, word) in words.iter().enumerate() {
                let folded = fold(word);
                if let Some((_, other)) = seen.iter().find(|(s, _)| *s == folded) {
                    let why = match case {
                        Case::Insensitive => {
                            " in any case (set `case = \"sensitive\"` to tell them apart)"
                        }
                        Case::Sensitive => "",
                    };
                    return Err(format!("`{other}` and `{word}` are the same spelling{why}"));
                }
                seen.push((folded, word));
                tags.push((g, f, k));
            }
        }
        spellings.push(expanded);
    }
    Ok(Some(Check::Consistent(Consistent {
        groups: groups.to_vec(),
        spellings,
        tags,
        case,
    })))
}

/// A form's brace expansions, with whitespace collapsed.
fn spellings_of(form: &str) -> Result<Vec<String>, String> {
    expand::expand(form)?
        .iter()
        .map(|w| {
            let w = prose::normalize(w);
            if w.is_empty() {
                return Err(format!("`{form}`: words can't be empty"));
            }
            Ok(w)
        })
        .collect()
}

fn list(forms: &[String]) -> String {
    let quoted: Vec<String> = forms.iter().map(|f| settings::quote(f)).collect();
    quoted.join(", ")
}

impl Consistent {
    pub(super) fn wants(&self) -> Vec<Want> {
        self.tags
            .iter()
            .map(|&(g, f, k)| Want::Literal(self.spellings[g][f][k].clone(), self.case))
            .collect()
    }

    /// The first form of each group a document uses wins; each spelling of
    /// another form after it is reported, suggesting the winner's.
    pub(super) fn document(&self, doc: &Doc<'_>, out: &mut Vec<FoundIn>) {
        let mut first: Vec<Option<usize>> = vec![None; self.groups.len()];
        for c in doc.candidates {
            let Some(&(g, f, k)) = self.tags.get(c.tag as usize) else {
                continue;
            };
            match first[g] {
                None => first[g] = Some(f),
                Some(winner) if winner != f => out.push(FoundIn {
                    block: c.block,
                    found: Found {
                        range: c.range.clone(),
                        args: vec![self.spellings[g][winner][k].clone()],
                    },
                }),
                Some(_) => {}
            }
        }
    }

    pub(super) fn describe(&self) -> Description {
        let mut lines: Vec<String> = self
            .groups
            .iter()
            .map(|forms| {
                let quoted: Vec<String> = forms.iter().map(|f| settings::quote(f)).collect();
                quoted.join(" or ")
            })
            .collect();
        let case = match self.case {
            Case::Insensitive => "in any case",
            Case::Sensitive => "in the case written",
        };
        lines.push(format!("matched {case}; the first a document uses wins"));
        Description {
            written: "consistent".into(),
            finds: lines.join("\n"),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::lint;
    use crate::rules::{EntryRef, compile_one};
    use crate::scan::{Format, Options};

    fn found(rule: &str, source: &str) -> Vec<(String, Vec<String>)> {
        let set = compile_one(rule).unwrap();
        lint::document(&set, Format::Markdown, source, Options::default())
            .issues
            .into_iter()
            .map(|i| (i.matched, i.args))
            .collect()
    }

    const RULE: &str = r#"schema = 1
        consistent = [["advisor{,s}", "adviser{,s}"], ["toward", "towards"]]"#;

    #[test]
    fn the_first_form_used_wins() {
        assert_eq!(
            found(
                RULE,
                "# Advisers\n\nAn advisor met two advisors, toward noon.\n\nTowards dusk, the adviser left."
            ),
            [
                ("advisor".into(), vec!["adviser".into()]),
                ("advisors".into(), vec!["advisers".into()]),
                ("Towards".into(), vec!["toward".into()]),
            ]
        );
        assert!(found(RULE, "An advisor and an advisor.").is_empty());
    }

    #[test]
    fn the_message_suggests_the_winner() {
        let set = compile_one(RULE).unwrap();
        let w = set.rules[0].wording(EntryRef::Check(0)).unwrap();
        assert_eq!(
            w.render("adviser", "R", &["advisor".into()]),
            "Prefer “advisor”, which this document uses first."
        );
    }

    #[test]
    fn exceptions_dont_count_and_scope_limits() {
        // The heading is out of scope and the first "adviser" is excepted,
        // so "advisor" comes first.
        let rule = format!("{RULE}\nexcept = [\"adviser\"]\nscope = [\"paragraph\"]");
        assert_eq!(
            found(
                &rule,
                "# Adviser\n\nThe adviser met an advisor, then advisers."
            ),
            [("advisers".into(), vec!["advisors".into()])]
        );
    }

    #[test]
    fn groups_must_make_sense() {
        let e =
            |groups: &str| compile_one(&format!("schema = 1\nconsistent = {groups}")).unwrap_err();
        assert!(
            e(r#"[["a"]]"#)
                .contains("`consistent`: each group needs two forms or more, not [\"a\"]")
        );
        assert!(e(r#"[["advisor{,s}", "adviser"]]"#).contains("expand to 2 and 1 spellings"));
        assert!(e(r#"[["Email", "email"]]"#).contains("same spelling in any case"));
        let ok = r#"schema = 1
            case = "sensitive"
            consistent = [["Email", "email"]]"#;
        assert_eq!(
            found(ok, "Email me. Or email."),
            [("email".into(), vec!["Email".into()])]
        );
        assert!(e(r#"[["a", " "]]"#).contains("`consistent`: ` `: words can't be empty"));
    }
}
