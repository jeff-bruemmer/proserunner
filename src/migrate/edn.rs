//! A minimal EDN reader, enough for the files proserunner 0.8 wrote:
//! maps, vectors, lists, sets, strings, keywords, symbols, numbers,
//! booleans, nil, `;` comments and `#_` discards.

#[derive(Debug, Clone, PartialEq)]
pub enum Edn {
    Nil,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Char(char),
    Keyword(String),
    Symbol(String),
    List(Vec<Edn>),
    Vector(Vec<Edn>),
    Set(Vec<Edn>),
    Map(Vec<(Edn, Edn)>),
}

impl Edn {
    /// A map value by keyword or string key: `get("name")` finds `:name`
    /// and `"name"`.
    pub fn get(&self, key: &str) -> Option<&Edn> {
        let Edn::Map(entries) = self else {
            return None;
        };
        entries.iter().find_map(|(k, v)| match k {
            Edn::Keyword(k) | Edn::Str(k) if k == key => Some(v),
            _ => None,
        })
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Edn::Str(s) => Some(s),
            _ => None,
        }
    }

    /// The items of a vector, list or set.
    pub fn items(&self) -> Option<&[Edn]> {
        match self {
            Edn::Vector(v) | Edn::List(v) | Edn::Set(v) => Some(v),
            _ => None,
        }
    }
}

/// How deeply forms may nest. 0.8's files nest a few levels; a limit
/// keeps a broken file from overflowing the stack.
const MAX_DEPTH: usize = 128;

/// Parses exactly one top-level form.
pub fn parse(src: &str) -> Result<Edn, String> {
    let mut r = Reader {
        chars: src.chars().collect(),
        pos: 0,
        depth: 0,
    };
    r.skip_space();
    let form = r.form()?.ok_or_else(|| r.error("expected a value"))?;
    r.skip_space();
    if r.pos < r.chars.len() {
        return Err(r.error("unexpected text after the value"));
    }
    Ok(form)
}

struct Reader {
    chars: Vec<char>,
    pos: usize,
    /// How many forms are being read, one inside another.
    depth: usize,
}

impl Reader {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn error(&self, message: &str) -> String {
        let before = &self.chars[..self.pos.min(self.chars.len())];
        let line = before.iter().filter(|&&c| c == '\n').count() + 1;
        let col = before.iter().rev().take_while(|&&c| c != '\n').count() + 1;
        format!("line {line}, column {col}: {message}")
    }

    fn skip_space(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_whitespace() || c == ',' {
                self.pos += 1;
            } else if c == ';' {
                while self.peek().is_some_and(|c| c != '\n') {
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }

    /// The next form, or `None` at a closing delimiter or the end.
    fn form(&mut self) -> Result<Option<Edn>, String> {
        if self.depth == MAX_DEPTH {
            return Err(self.error(&format!("nested more than {MAX_DEPTH} levels deep")));
        }
        self.depth += 1;
        let form = self.read_form();
        self.depth -= 1;
        form
    }

    fn read_form(&mut self) -> Result<Option<Edn>, String> {
        loop {
            self.skip_space();
            let Some(c) = self.peek() else {
                return Ok(None);
            };
            return Ok(Some(match c {
                ')' | ']' | '}' => return Ok(None),
                '(' => Edn::List(self.seq(')')?),
                '[' => Edn::Vector(self.seq(']')?),
                '{' => self.map()?,
                '"' => Edn::Str(self.string()?),
                '\\' => self.character()?,
                ':' => {
                    self.pos += 1;
                    Edn::Keyword(self.token())
                }
                '#' => match self.chars.get(self.pos + 1) {
                    Some('{') => {
                        self.pos += 1;
                        Edn::Set(self.seq('}')?)
                    }
                    Some('_') => {
                        self.pos += 2;
                        self.form()?
                            .ok_or_else(|| self.error("#_ needs a value to discard"))?;
                        continue;
                    }
                    _ => return Err(self.error("tagged values (#tag) aren't supported")),
                },
                _ => self.atom()?,
            }));
        }
    }

    fn seq(&mut self, close: char) -> Result<Vec<Edn>, String> {
        self.pos += 1;
        let mut items = Vec::new();
        while let Some(item) = self.form()? {
            items.push(item);
        }
        if self.peek() != Some(close) {
            return Err(self.error(&format!("expected `{close}`")));
        }
        self.pos += 1;
        Ok(items)
    }

    fn map(&mut self) -> Result<Edn, String> {
        let items = self.seq('}')?;
        if items.len() % 2 != 0 {
            return Err(self.error("a map needs an even number of forms"));
        }
        let mut entries = Vec::new();
        let mut it = items.into_iter();
        while let (Some(k), Some(v)) = (it.next(), it.next()) {
            entries.push((k, v));
        }
        Ok(Edn::Map(entries))
    }

    fn string(&mut self) -> Result<String, String> {
        self.pos += 1;
        let mut s = String::new();
        loop {
            let c = self
                .peek()
                .ok_or_else(|| self.error("unterminated string"))?;
            self.pos += 1;
            match c {
                '"' => return Ok(s),
                '\\' => {
                    let e = self
                        .peek()
                        .ok_or_else(|| self.error("unterminated string"))?;
                    self.pos += 1;
                    match e {
                        'n' => s.push('\n'),
                        't' => s.push('\t'),
                        'r' => s.push('\r'),
                        'b' => s.push('\u{8}'),
                        'f' => s.push('\u{c}'),
                        '"' | '\\' => s.push(e),
                        'u' => {
                            let hex: String = self.chars.iter().skip(self.pos).take(4).collect();
                            let code = u32::from_str_radix(&hex, 16)
                                .ok()
                                .and_then(char::from_u32)
                                .ok_or_else(|| self.error("invalid \\u escape"))?;
                            self.pos += 4;
                            s.push(code);
                        }
                        _ => return Err(self.error(&format!("unknown escape \\{e}"))),
                    }
                }
                c => s.push(c),
            }
        }
    }

    fn character(&mut self) -> Result<Edn, String> {
        self.pos += 1;
        let name = self.token();
        let c = match name.as_str() {
            "newline" => '\n',
            "space" => ' ',
            "tab" => '\t',
            "return" => '\r',
            n if n.chars().count() == 1 => n.chars().next().unwrap_or(' '),
            "" => {
                let c = self
                    .peek()
                    .ok_or_else(|| self.error("expected a character"))?;
                self.pos += 1;
                c
            }
            _ => return Err(self.error(&format!("unknown character \\{name}"))),
        };
        Ok(Edn::Char(c))
    }

    fn token(&mut self) -> String {
        let start = self.pos;
        while self
            .peek()
            .is_some_and(|c| c.is_alphanumeric() || ".*+!-_?$%&=<>/'#:".contains(c))
        {
            self.pos += 1;
        }
        self.chars[start..self.pos].iter().collect()
    }

    fn atom(&mut self) -> Result<Edn, String> {
        let token = self.token();
        if token.is_empty() {
            return Err(self.error(&format!("unexpected `{}`", self.peek().unwrap_or(' '))));
        }
        Ok(match token.as_str() {
            "nil" => Edn::Nil,
            "true" => Edn::Bool(true),
            "false" => Edn::Bool(false),
            t => {
                if let Ok(i) = t.parse::<i64>() {
                    Edn::Int(i)
                } else if let Ok(f) = t.parse::<f64>() {
                    Edn::Float(f)
                } else {
                    Edn::Symbol(t.to_owned())
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_checks() {
        let e = parse(
            r#";; comment
            {:name "hedging", :kind "existence"
             :specimens ["I think" "so to speak"] #_ :ignored
             :n 3 :ok true :none nil}"#,
        )
        .unwrap();
        assert_eq!(e.get("name").and_then(Edn::as_str), Some("hedging"));
        assert_eq!(
            e.get("specimens").and_then(Edn::items).map(<[Edn]>::len),
            Some(2)
        );
        assert_eq!(e.get("n"), Some(&Edn::Int(3)));
        assert_eq!(e.get("none"), Some(&Edn::Nil));
    }

    #[test]
    fn reads_sets_and_escapes() {
        let e = parse(r#"#{"a \"b\"" "c\\d" "é"}"#).unwrap();
        assert_eq!(
            e,
            Edn::Set(vec![
                Edn::Str("a \"b\"".into()),
                Edn::Str("c\\d".into()),
                Edn::Str("é".into())
            ])
        );
    }

    #[test]
    fn errors_have_positions() {
        let e = parse("{:a\n \"b").unwrap_err();
        assert!(e.contains("unterminated"), "{e}");
        let e = parse("[1 2").unwrap_err();
        assert!(e.starts_with("line 1"), "{e}");
    }

    #[test]
    fn deep_nesting_is_an_error_not_a_crash() {
        let e = parse(&"[".repeat(100_000)).unwrap_err();
        assert!(e.contains("nested more than 128"), "{e}");
        let e = parse(&"#_ ".repeat(100_000)).unwrap_err();
        assert!(e.contains("nested more than 128"), "{e}");
        assert!(parse(&format!("{}{}", "[".repeat(100), "]".repeat(100))).is_ok());
    }

    #[test]
    fn empty_collections() {
        assert_eq!(parse("#{}").unwrap(), Edn::Set(vec![]));
        assert_eq!(
            parse("{:ignore #{} :ignore-issues #{}}")
                .unwrap()
                .get("ignore"),
            Some(&Edn::Set(vec![]))
        );
    }
}
