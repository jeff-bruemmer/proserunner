//! Any rule file compiles or is rejected with an error, and a compiled
//! rule lints, explains and tests its examples without panicking.

#![no_main]

use libfuzzer_sys::fuzz_target;
use proserunner::rules::{RuleSet, RuleSource};
use proserunner::scan::{DOCUMENTS, Options};
use proserunner::{examples, lint};

const SAMPLE: &str = "# A *very* unique title\n\nWe utilize synergy, the the end. In order to win, etc.\n\n- naïve café\n\n## An advisor's Guide\n\nThe adviser used an API, an application programming interface (API), and PDFs toward noon.\n";

fuzz_target!(|data: &[u8]| {
    let Ok(text) = str::from_utf8(data) else {
        return;
    };
    let set = RuleSet::compile([RuleSource::new("fuzz/rule", text)]).rules;
    for rule in set.rules() {
        for entry in 0..rule.entries.len() {
            let _ = rule.forms(entry);
        }
    }
    for document in DOCUMENTS {
        let linted = lint::document(&set, document.format, SAMPLE, Options::default());
        for issue in &linted.issues {
            assert!(SAMPLE.get(issue.span.clone()).is_some(), "{:?}", issue.span);
        }
    }
    let _ = examples::run(&set);
});
