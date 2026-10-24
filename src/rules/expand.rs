//! Brace expansion for `words`: `synerg{y,ies}` is `synergy` and
//! `synergies`. Expansion happens at load, so entries stay literal.

/// Expanding past this many words is almost certainly a mistake.
const MAX_EXPANSIONS: usize = 1000;

/// Expands every `{a,b,...}` group in `s`. A group needs a comma; braces
/// without one are literal. `\{`, `\}`, `\,` and `\\` escape.
pub(crate) fn expand(s: &str) -> Result<Vec<String>, String> {
    let mut out = vec![String::new()];
    let mut chars = s.char_indices().peekable();

    while let Some((i, c)) = chars.next() {
        let literal = match c {
            '\\' if chars
                .peek()
                .is_some_and(|&(_, n)| matches!(n, '{' | '}' | ',' | '\\')) =>
            {
                chars.next().map_or('\\', |(_, n)| n)
            }
            '{' => match group(&s[i + 1..])? {
                Some((alternatives, consumed)) => {
                    for _ in 0..consumed {
                        chars.next();
                    }
                    out = out
                        .iter()
                        .flat_map(|o| alternatives.iter().map(move |a| format!("{o}{a}")))
                        .collect();
                    if out.len() > MAX_EXPANSIONS {
                        return Err(format!("`{s}` expands to more than {MAX_EXPANSIONS} words"));
                    }
                    continue;
                }
                None => '{',
            },
            c => c,
        };
        for o in &mut out {
            o.push(literal);
        }
    }
    Ok(out)
}

/// Parses a group after its `{`. Returns its alternatives and how many
/// chars it used, including the `}`, or `None` if it isn't a group.
fn group(rest: &str) -> Result<Option<(Vec<String>, usize)>, String> {
    let mut alternatives = vec![String::new()];
    let mut chars = rest.chars();
    let mut used = 0;

    while let Some(c) = chars.next() {
        used += 1;
        match c {
            '\\' => {
                if let Some(n) = chars.next() {
                    used += 1;
                    if let Some(last) = alternatives.last_mut() {
                        if !matches!(n, '{' | '}' | ',' | '\\') {
                            last.push('\\');
                        }
                        last.push(n);
                    }
                }
            }
            '{' => return Err("nested braces aren't supported; write the words out".into()),
            ',' => alternatives.push(String::new()),
            '}' => {
                return Ok((alternatives.len() > 1).then_some((alternatives, used)));
            }
            c => {
                if let Some(last) = alternatives.last_mut() {
                    last.push(c);
                }
            }
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::expand;

    #[test]
    fn expands_groups() {
        assert_eq!(expand("synerg{y,ies}").unwrap(), ["synergy", "synergies"]);
        assert_eq!(
            expand("{general ,}consensus").unwrap(),
            ["general consensus", "consensus"]
        );
    }

    #[test]
    fn expands_several_groups() {
        assert_eq!(expand("{a,b}{c,d}").unwrap(), ["ac", "ad", "bc", "bd"]);
    }

    #[test]
    fn braces_without_a_comma_are_literal() {
        assert_eq!(expand("{x}").unwrap(), ["{x}"]);
        assert_eq!(expand("a { b").unwrap(), ["a { b"]);
    }

    #[test]
    fn escapes() {
        assert_eq!(expand(r"\{a,b\}").unwrap(), ["{a,b}"]);
        assert_eq!(expand(r"{a\,b,c}").unwrap(), ["a,b", "c"]);
        assert_eq!(expand(r"C:\path").unwrap(), [r"C:\path"]);
    }

    #[test]
    fn nested_is_an_error() {
        assert!(expand("{a,{b,c}}").is_err());
    }
}
