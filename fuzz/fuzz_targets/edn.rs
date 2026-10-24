//! Any 0.8 check file converts or is rejected with an error. Migration
//! reads these files on its own, so a bad one mustn't crash a run.

#![no_main]

use libfuzzer_sys::fuzz_target;
use proserunner::migrate::{checks, edn};

fuzz_target!(|data: &[u8]| {
    let Ok(text) = str::from_utf8(data) else {
        return;
    };
    let _ = edn::parse(text);
    let _ = checks::convert(text);
});
