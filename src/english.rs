//! English phrasing for messages: counts of things, and lists.

/// "No issues found.", "Found 1 issue." or "Found N issues."
pub fn summary(count: usize) -> String {
    match count {
        0 => "No issues found.".into(),
        1 => "Found 1 issue.".into(),
        n => format!("Found {n} issues."),
    }
}

/// `1 issue`, `2 issues`.
pub fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

/// `a`, `a and b`, `a, b and c`.
pub fn and_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}
