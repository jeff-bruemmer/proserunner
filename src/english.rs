//! English phrasing for messages: counts of things, and lists.

/// What a run checked and found: "Checked 3 files and found 5 issues.",
/// or "Checked 1 file and found no issues." `levels` is the issues by
/// level, such as `["1 error", "4 warnings"]`; with two or more, the
/// summary lists them.
pub fn summary(files: usize, issues: usize, levels: &[String]) -> String {
    if files == 0 {
        return "Checked no files.".into();
    }
    let found = match issues {
        0 => "no issues".into(),
        n => count(n, "issue"),
    };
    let by_level = if levels.len() > 1 {
        format!(": {}", and_list(levels))
    } else {
        String::new()
    };
    format!(
        "Checked {} and found {found}{by_level}.",
        count(files, "file")
    )
}

/// `1 issue`, `2 issues`, `5,307 issues`.
pub fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{} {noun}s", number(n))
    }
}

/// `5,307`: digits in groups of three.
pub fn number(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// `a`, `a and b`, `a, b and c`.
pub fn and_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summaries_count_files_and_issues() {
        assert_eq!(summary(0, 0, &[]), "Checked no files.");
        assert_eq!(summary(1, 0, &[]), "Checked 1 file and found no issues.");
        assert_eq!(
            summary(2, 1, &["1 hint".into()]),
            "Checked 2 files and found 1 issue."
        );
        assert_eq!(
            summary(3, 5_307, &["7 errors".into(), "5,300 warnings".into()]),
            "Checked 3 files and found 5,307 issues: 7 errors and 5,300 warnings."
        );
    }

    #[test]
    fn numbers_group_thousands() {
        assert_eq!(number(0), "0");
        assert_eq!(number(999), "999");
        assert_eq!(number(1_000), "1,000");
        assert_eq!(number(5_307), "5,307");
        assert_eq!(number(1_234_567), "1,234,567");
        assert_eq!(count(4_791, "issue"), "4,791 issues");
    }
}
