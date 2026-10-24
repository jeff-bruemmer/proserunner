//! The `Check` enum: one variant per check, and the match arms that
//! dispatch each hook to its check.

use super::acronyms::{self, Acronyms};
use super::capitalization::{self, Capitalization};
use super::consistent::{self, Consistent};
use super::hooks::{Doc, Found, FoundIn, Want};
use super::limit::{self, Limit};
use super::metric::{self, Metric};
use super::repeat;
use super::spec::{Description, Spec};
use crate::rules::rule_file::RuleFile;
use crate::scan::Block;

/// Every check's spec, in the order `compile` turns them on.
const SPECS: [&Spec; 6] = [
    &repeat::SPEC,
    &consistent::SPEC,
    &limit::SPEC,
    &capitalization::SPEC,
    &acronyms::SPEC,
    &metric::SPEC,
];

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Check {
    Repeat,
    Consistent(Consistent),
    Limit(Limit),
    Capitalization(Capitalization),
    Acronyms(Acronyms),
    Metric(Metric),
}

impl Check {
    pub(crate) fn spec(&self) -> &'static Spec {
        match self {
            Check::Repeat => &repeat::SPEC,
            Check::Consistent(_) => &consistent::SPEC,
            Check::Limit(_) => &limit::SPEC,
            Check::Capitalization(_) => &capitalization::SPEC,
            Check::Acronyms(_) => &acronyms::SPEC,
            Check::Metric(_) => &metric::SPEC,
        }
    }

    /// What the token tier finds for it, in tag order.
    pub(crate) fn wants(&self) -> Vec<Want> {
        match self {
            Check::Consistent(c) => c.wants(),
            Check::Acronyms(a) => a.wants(),
            Check::Repeat | Check::Limit(_) | Check::Capitalization(_) | Check::Metric(_) => {
                Vec::new()
            }
        }
    }

    /// Appends what the check finds in `block` of `source`.
    pub(crate) fn block(&self, block: &Block, source: &str, out: &mut Vec<Found>) {
        match self {
            Check::Repeat => repeat::block(block, source, out),
            Check::Limit(l) => l.block(block, out),
            Check::Capitalization(c) => c.block(block, out),
            Check::Metric(m) => m.block(block, out),
            Check::Consistent(_) | Check::Acronyms(_) => {}
        }
    }

    /// Whether it has a `document` hook.
    pub(crate) fn is_document(&self) -> bool {
        match self {
            Check::Consistent(_) | Check::Acronyms(_) => true,
            Check::Metric(m) => m.is_document(),
            Check::Repeat | Check::Limit(_) | Check::Capitalization(_) => false,
        }
    }

    /// Appends what the check finds across a document, each with its
    /// block.
    pub(crate) fn document(&self, doc: &Doc<'_>, out: &mut Vec<FoundIn>) {
        match self {
            Check::Consistent(c) => c.document(doc, out),
            Check::Acronyms(a) => a.document(doc, out),
            Check::Metric(m) => m.document(doc, out),
            Check::Repeat | Check::Limit(_) | Check::Capitalization(_) => {}
        }
    }

    /// For `explain`.
    pub(crate) fn describe(&self) -> Description {
        match self {
            Check::Repeat => repeat::describe(),
            Check::Consistent(c) => c.describe(),
            Check::Limit(l) => l.describe(),
            Check::Capitalization(c) => c.describe(),
            Check::Acronyms(a) => a.describe(),
            Check::Metric(m) => m.describe(),
        }
    }
}

/// Every check's key, in a fixed order.
pub(crate) fn keys() -> impl Iterator<Item = &'static str> {
    SPECS.iter().map(|s| s.key)
}

/// The checks `file` turns on, in [`keys`] order. Errors name the check.
pub(crate) fn compile(file: &RuleFile) -> Result<Vec<Check>, String> {
    let checks = [
        repeat::compile(file.repeat),
        consistent::compile(&file.consistent, file.case).map_err(named(&consistent::SPEC))?,
        limit::compile(file.limit.as_ref()).map_err(named(&limit::SPEC))?,
        capitalization::compile(file.capitalization.as_ref())
            .map_err(named(&capitalization::SPEC))?,
        acronyms::compile(file.acronyms.as_ref()),
        metric::compile(file.metric.as_ref()).map_err(named(&metric::SPEC))?,
    ];
    Ok(checks.into_iter().flatten().collect())
}

/// Prefixes an error with the check's key.
fn named(spec: &'static Spec) -> impl Fn(String) -> String {
    move |e| format!("`{}`: {e}", spec.key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::Template;

    #[test]
    fn specs_are_in_variant_order() {
        let keys: Vec<&str> = keys().collect();
        assert_eq!(
            keys,
            [
                "repeat",
                "consistent",
                "limit",
                "capitalization",
                "acronyms",
                "metric"
            ]
        );
        assert_eq!(Check::Repeat.spec().key, "repeat");
    }

    #[test]
    fn suggestions_use_their_vars() {
        for spec in SPECS {
            for s in spec.suggest {
                Template::suggestion(s, spec.vars).unwrap();
            }
            Template::parse(spec.message, spec.vars).unwrap();
        }
    }
}
