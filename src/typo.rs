//! Did-you-mean hints for names people type: rule ids, directive actions.

/// The id closest to a misspelled one, if any is close.
pub(crate) fn closest<'a>(id: &str, ids: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    closest_within(id, ids, 3)
}

/// The id closest to `id`, if any is at most `max` edits away.
pub(crate) fn closest_within<'a>(
    id: &str,
    ids: impl Iterator<Item = &'a str>,
    max: usize,
) -> Option<&'a str> {
    ids.map(|c| (distance(id, c), c))
        .filter(|&(d, _)| d <= max)
        .min()
        .map(|(_, c)| c)
}

/// Levenshtein distance, by chars.
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, &cb) in b.iter().enumerate() {
            let cur = row[j + 1];
            row[j + 1] = (prev + usize::from(ca != cb)).min(row[j] + 1).min(cur + 1);
            prev = cur;
        }
    }
    row[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_distance() {
        assert_eq!(distance("kitten", "sitting"), 3);
        assert_eq!(distance("", "abc"), 3);
        assert_eq!(
            closest(
                "default/clichés",
                ["default/cliches", "default/jargon"].into_iter()
            ),
            Some("default/cliches")
        );
        assert_eq!(closest("zzz", ["default/jargon"].into_iter()), None);
    }
}
