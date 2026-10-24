//! Ignore lists: `ignores.toml`, one per scope, written by proserunner.
//!
//! ```toml
//! schema = 1
//! accept = ["hopefully"]           # whole matches to ignore everywhere
//!
//! [[issue]]                         # one rule's match in one file
//! file = "docs/api.md"
//! rule = "default/pompous-diction"
//! match = "utilize"
//! line = 42                         # optional: only on this line
//! ```
//!
//! Matches compare like `accept`: whole, ignoring case and runs of
//! whitespace. An `[[issue]]` without `rule` applies to every rule.
//!
//! A relative `file` is relative to the project root in a project's list,
//! and to the working directory in the global list. proserunner writes
//! global entries with absolute (`~/…`) paths, so they work from anywhere;
//! a relative one, such as a migrated 0.8 entry, is never stale, since
//! whether its file exists depends on where a run is.
//!
//! Edits go through `toml_edit`, so comments people add survive, and a
//! file that doesn't parse is never written: the edit fails instead.

use std::collections::HashMap;
use std::fs;
use std::io::ErrorKind;
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use toml_edit::{Array, ArrayOfTables, DocumentMut, Item, Table, Value};

use crate::ctx::{self, Ctx, ListScope};
use crate::error::{Error, Result};
use crate::fsutil;
use crate::prose;
use crate::scan;
use crate::settings;

/// An ignore list's name, in the user config directory and in
/// `.proserunner/`.
pub const FILE: &str = "ignores.toml";

/// The only ignore-file format this version reads.
pub const SCHEMA: u32 = 1;

/// The lock file that serializes edits: see [`lock`].
const LOCK: &str = ".lock";

const NEW_FILE: &str = "\
# Written by `proserunner ignore` and `--ignore-issues`. Comments you add
# are kept.
schema = 1
";

/// A proserunner ignore list, written by `proserunner ignore` and
/// `--ignore-issues`.
#[derive(Debug, Default, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[cfg_attr(test, schemars(title = "proserunner ignore list"))]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct IgnoreFile {
    /// The format version: 1.
    #[cfg_attr(test, schemars(extend("const" = 1)))]
    pub schema: u32,
    /// Matches never reported, compared whole and in any case.
    #[serde(default)]
    pub accept: Vec<String>,
    /// Matches not reported in one file.
    #[serde(default)]
    pub issue: Vec<Entry>,
}

impl IgnoreFile {
    /// How many entries it has: `accept` and `[[issue]]` together.
    pub fn len(&self) -> usize {
        self.accept.len() + self.issue.len()
    }

    /// True when it has no entries.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// One `[[issue]]`: a match not to report in one file.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct Entry {
    /// The file: relative to the project root in a project's list, and to
    /// the working directory in the global one.
    pub file: String,
    /// The rule's id. Without one, the match is ignored for every rule.
    pub rule: Option<String>,
    /// The match, compared whole and in any case.
    #[serde(rename = "match")]
    pub matched: String,
    /// Only on this line.
    pub line: Option<usize>,
}

impl Entry {
    /// An entry for `matched` anywhere in `file`.
    pub fn new(file: String, rule: Option<String>, matched: String) -> Self {
        Entry {
            file,
            rule,
            matched,
            line: None,
        }
    }

    /// Where the entry applies: `file`, or `file:line`.
    pub fn place(&self) -> String {
        match self.line {
            Some(line) => format!("{}:{line}", self.file),
            None => self.file.clone(),
        }
    }

    fn same(&self, other: &Entry) -> bool {
        self.file == other.file
            && self.rule == other.rule
            && self.line == other.line
            && key(&self.matched) == key(&other.matched)
    }
}

/// How matches compare: whitespace collapsed, case ignored, and hidden
/// text the same whether it's shown as "…" or held as a placeholder.
pub fn key(s: &str) -> String {
    prose::normalize(s)
        .to_lowercase()
        .replace(scan::PLACEHOLDER, "…")
}

/// Parses an ignore list's text and checks it. The error is a message for
/// [`Error::config`].
pub fn parse(text: &str) -> Result<IgnoreFile, String> {
    let file: IgnoreFile = settings::parse(text)?;
    settings::check_schema(file.schema, SCHEMA)?;
    if file.accept.iter().any(|a| a.trim().is_empty()) {
        return Err("`accept` entries can't be empty".into());
    }
    if let Some(e) = file
        .issue
        .iter()
        .find(|e| e.file.trim().is_empty() || e.matched.trim().is_empty())
    {
        return Err(format!(
            "every [[issue]] needs a `file` and a `match` (found file = {:?}, match = {:?})",
            e.file, e.matched
        ));
    }
    Ok(file)
}

/// Where a scope keeps its list.
pub fn path(ctx: &Ctx, scope: &ListScope) -> Result<PathBuf> {
    Ok(scope.dir(ctx)?.join(FILE))
}

/// A scope's list, or `None` when it has none.
pub fn read(ctx: &Ctx, scope: &ListScope) -> Result<Option<IgnoreFile>> {
    let path = path(ctx, scope)?;
    match fs::read_to_string(&path) {
        Ok(text) => parse(&text)
            .map(Some)
            .map_err(|m| Error::config(ctx.display(&path), m)),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error::io(&path, e)),
    }
}

/// The absolute path an entry's `file` names.
pub fn resolve(ctx: &Ctx, scope: &ListScope, file: &str) -> PathBuf {
    let base = match scope {
        ListScope::Project(root) => root.as_path(),
        ListScope::Global => ctx.cwd.as_path(),
    };
    ctx.resolve(base, file)
}

/// How a new entry names the file at absolute `path`: relative to the
/// root in a project (when the file is inside it), otherwise `~/…` or
/// absolute. Inside means a whole-component prefix, so `/x/proj-old` isn't
/// inside `/x/proj`.
pub fn file_for(ctx: &Ctx, scope: &ListScope, path: &Path) -> String {
    match scope {
        ListScope::Project(root) => match path.strip_prefix(root) {
            Ok(rel) => ctx::slashed(rel),
            Err(_) => ctx.portable(path),
        },
        ListScope::Global => ctx.portable(path),
    }
}

/// Entries whose file no longer exists.
pub fn stale<'a>(ctx: &Ctx, scope: &ListScope, list: &'a IgnoreFile) -> Vec<&'a Entry> {
    list.issue
        .iter()
        .filter(|e| is_stale(ctx, scope, e))
        .collect()
}

/// Whether an entry's file no longer exists. A relative `file` in the
/// global list never is: it's relative to wherever a run is, so it may be
/// there from another directory.
pub fn is_stale(ctx: &Ctx, scope: &ListScope, entry: &Entry) -> bool {
    let absolute = entry.file.starts_with("~/") || Path::new(&entry.file).is_absolute();
    if *scope == ListScope::Global && !absolute {
        return false;
    }
    !resolve(ctx, scope, &entry.file).exists()
}

// ---------------------------------------------------------------------
// Editing.

/// A scope's list, open for editing. Methods keep the parsed list and the
/// TOML document in step.
#[derive(Debug)]
pub struct Store {
    path: PathBuf,
    list: IgnoreFile,
    doc: DocumentMut,
    changed: bool,
}

/// Runs `f` on a scope's list while holding the lock, then saves the list
/// if `f` changed it.
pub fn edit<T>(ctx: &Ctx, scope: &ListScope, f: impl FnOnce(&mut Store) -> Result<T>) -> Result<T> {
    let path = path(ctx, scope)?;
    let _lock = lock(ctx, scope)?;
    let mut store = Store::open(&path, &ctx.display(&path))?;
    let out = f(&mut store)?;
    if store.changed {
        fsutil::write_atomic(&store.path, &store.doc.to_string())?;
    }
    Ok(out)
}

/// Waits for the lock that serializes edits to `scope`'s settings. It's
/// in the global config directory, so a project gets no lock file, unless
/// that directory can't be written (a read-only home): then a project's
/// edits lock in its own.
pub(crate) fn lock(ctx: &Ctx, scope: &ListScope) -> Result<fsutil::Lock> {
    let own = || fsutil::lock(&scope.dir(ctx)?.join(LOCK));
    match (scope, &ctx.config_dir) {
        (ListScope::Project(_), Some(dir)) => fsutil::lock(&dir.join(LOCK)).or_else(|_| own()),
        _ => own(),
    }
}

impl Store {
    fn open(path: &Path, origin: &str) -> Result<Store> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == ErrorKind::NotFound => NEW_FILE.to_owned(),
            Err(e) => return Err(Error::io(path, e)),
        };
        let list = parse(&text).map_err(|m| Error::config(origin, m))?;
        let doc = text
            .parse::<DocumentMut>()
            .map_err(|e| Error::config(origin, e.to_string().trim_end().to_owned()))?;
        Ok(Store {
            path: path.to_path_buf(),
            list,
            doc,
            changed: false,
        })
    }

    /// Adds a whole match to ignore everywhere. False if it's already there.
    pub fn add_accept(&mut self, word: &str) -> bool {
        let word = prose::normalize(word);
        if self.list.accept.iter().any(|a| key(a) == key(&word)) {
            return false;
        }
        if self.doc.get("accept").and_then(Item::as_array).is_none() {
            self.doc.insert("accept", toml_edit::value(Array::new()));
        }
        if let Some(array) = self.doc.get_mut("accept").and_then(Item::as_array_mut) {
            array.push(word.as_str());
        }
        self.list.accept.push(word);
        self.changed = true;
        true
    }

    /// Removes a whole match. False if it wasn't there.
    pub fn remove_accept(&mut self, word: &str) -> bool {
        let k = key(word);
        let before = self.list.accept.len();
        self.list.accept.retain(|a| key(a) != k);
        if self.list.accept.len() == before {
            return false;
        }
        if let Some(array) = self.doc.get_mut("accept").and_then(Item::as_array_mut) {
            array.retain(|v| v.as_str().is_none_or(|s| key(s) != k));
        }
        self.changed = true;
        true
    }

    /// Adds an issue entry. False if the same entry is already there.
    pub fn add_issue(&mut self, entry: Entry) -> bool {
        if self.list.issue.iter().any(|e| e.same(&entry)) {
            return false;
        }
        if let Some(tables) = self.issue_tables() {
            tables.push(table_for(&entry));
        }
        self.list.issue.push(entry);
        self.changed = true;
        true
    }

    /// Keeps the issue entries `keep` accepts; returns how many went.
    pub fn retain_issues(&mut self, keep: impl Fn(&Entry) -> bool) -> usize {
        let keeps: Vec<bool> = self.list.issue.iter().map(keep).collect();
        let removed = keeps.iter().filter(|k| !**k).count();
        if removed == 0 {
            return 0;
        }
        if let Some(tables) = self.issue_tables() {
            // The tables are the parsed entries, in the same order.
            let mut i = 0;
            tables.retain(|_| {
                let k = keeps.get(i).copied().unwrap_or(true);
                i += 1;
                k
            });
        }
        let mut i = 0;
        self.list.issue.retain(|_| {
            let k = keeps[i];
            i += 1;
            k
        });
        self.changed = true;
        removed
    }

    /// Removes everything; returns how many entries went.
    pub fn clear(&mut self) -> usize {
        let n = self.list.len();
        if n > 0 {
            self.doc.remove("accept");
            self.doc.remove("issue");
            self.list.accept.clear();
            self.list.issue.clear();
            self.changed = true;
        }
        n
    }

    /// The `[[issue]]` tables, made an array of tables if they were
    /// written inline (`issue = [{ … }]`).
    fn issue_tables(&mut self) -> Option<&mut ArrayOfTables> {
        if self
            .doc
            .get("issue")
            .and_then(Item::as_array_of_tables)
            .is_none()
        {
            let tables = match self.doc.remove("issue") {
                None | Some(Item::None) => ArrayOfTables::new(),
                // toml_edit won't convert an empty array.
                Some(Item::Value(Value::Array(a))) if a.is_empty() => ArrayOfTables::new(),
                Some(item) => match item.into_array_of_tables() {
                    Ok(tables) => tables,
                    Err(item) => {
                        self.doc.insert("issue", item);
                        return None;
                    }
                },
            };
            self.doc.insert("issue", Item::ArrayOfTables(tables));
        }
        self.doc
            .get_mut("issue")
            .and_then(Item::as_array_of_tables_mut)
    }
}

fn table_for(entry: &Entry) -> Table {
    let mut t = Table::new();
    t.insert("file", toml_edit::value(entry.file.as_str()));
    if let Some(rule) = &entry.rule {
        t.insert("rule", toml_edit::value(rule.as_str()));
    }
    t.insert("match", toml_edit::value(entry.matched.as_str()));
    if let Some(line) = entry.line {
        t.insert("line", toml_edit::value(line as i64));
    }
    t
}

// ---------------------------------------------------------------------
// Applying.

/// Every ignore that applies to a run, with files resolved.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct Ignores {
    /// Keys (see [`key`]), each with the list it's in, as shown.
    pub accept: HashMap<String, String>,
    /// `[[issue]]` entries, by the absolute path of their file.
    pub issues: HashMap<PathBuf, Vec<Ignored>>,
}

/// An `[[issue]]` entry, filed under its file's absolute path.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Ignored {
    /// The rule's id; `None` for every rule.
    pub rule: Option<String>,
    /// A key (see [`key`]).
    pub matched: String,
    /// Only on this line.
    pub line: Option<usize>,
    /// The list it's in, as shown.
    pub origin: String,
}

/// The lists of `scopes`, merged.
pub fn load(ctx: &Ctx, scopes: &[ListScope]) -> Result<Ignores> {
    let mut ignores = Ignores::default();
    for scope in scopes {
        let Some(list) = read(ctx, scope)? else {
            continue;
        };
        let origin = ctx.display(&path(ctx, scope)?);
        ignores
            .accept
            .extend(list.accept.iter().map(|a| (key(a), origin.clone())));
        for e in list.issue {
            ignores
                .issues
                .entry(resolve(ctx, scope, &e.file))
                .or_default()
                .push(Ignored {
                    rule: e.rule,
                    matched: key(&e.matched),
                    line: e.line,
                    origin: origin.clone(),
                });
        }
    }
    Ok(ignores)
}

/// `1,3,5-7` as sorted ranges that don't overlap. Ranges stay ranges, so
/// `1-99999999999` costs nothing.
pub fn parse_numbers(s: &str) -> Result<Vec<RangeInclusive<usize>>, String> {
    let number = |t: &str| -> Result<usize, String> {
        match t.trim().parse::<usize>() {
            Ok(0) | Err(_) => Err(format!(
                "`{}` isn't an issue number: use numbers from 1, like 1,3,5-7",
                t.trim()
            )),
            Ok(n) => Ok(n),
        }
    };
    let mut ranges = Vec::new();
    let parts = s.split(',').map(str::trim).filter(|p| !p.is_empty());
    for part in parts {
        match part.split_once('-') {
            Some((a, b)) => {
                let (a, b) = (number(a)?, number(b)?);
                if a > b {
                    return Err(format!("`{part}`: a range goes from low to high"));
                }
                ranges.push(a..=b);
            }
            None => {
                let n = number(part)?;
                ranges.push(n..=n);
            }
        }
    }
    if ranges.is_empty() {
        return Err("no issue numbers given".into());
    }
    ranges.sort_unstable_by_key(|r| *r.start());
    let mut out: Vec<RangeInclusive<usize>> = Vec::with_capacity(ranges.len());
    for r in ranges {
        match out.last_mut() {
            Some(last) if *r.start() <= last.end().saturating_add(1) => {
                *last = *last.start()..=*last.end().max(r.end());
            }
            _ => out.push(r),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(dir: &Path) -> Ctx {
        Ctx::new(dir.join("work"), Some(dir.join("home")), None, false)
    }

    fn entry(file: &str, matched: &str) -> Entry {
        Entry {
            file: file.into(),
            rule: Some("default/jargon".into()),
            matched: matched.into(),
            line: None,
        }
    }

    #[test]
    fn keys_ignore_case_spacing_and_how_hidden_text_is_shown() {
        assert_eq!(key("Due to\n the  FACT"), "due to the fact");
        assert_eq!(key("See \u{1A} for"), key("See … for"));
    }

    #[test]
    fn strict_parsing() {
        assert!(
            parse("schema = 1\naccep = []")
                .unwrap_err()
                .contains("unknown field `accep`")
        );
        assert!(
            parse("schema = 1\n[[issue]]\nfile = \"a.md\"")
                .unwrap_err()
                .contains("match")
        );
        assert!(
            parse("schema = 1\n[[issue]]\nfile = \"\"\nmatch = \"x\"")
                .unwrap_err()
                .contains("needs a `file`")
        );
        assert!(parse("schema = 3").unwrap_err().contains("schema 3"));
        parse("schema = 1\nissue = [{ file = \"a.md\", match = \"x\" }]").unwrap();
    }

    #[test]
    fn edits_keep_comments_and_reparse() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = ctx(dir.path());
        let scope = ListScope::Global;
        let path = path(&ctx, &scope).unwrap();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            "schema = 1\n# Product names.\naccept = [\"Kubernetes\"]\n",
        )
        .unwrap();

        edit(&ctx, &scope, |s| {
            assert!(s.add_accept("hopefully"));
            assert!(!s.add_accept("KUBERNETES"));
            assert!(s.add_issue(entry("~/a.md", "utilize")));
            assert!(!s.add_issue(entry("~/a.md", "Utilize")));
            assert!(s.add_issue(entry("~/b.md", "leverage")));
            Ok(())
        })
        .unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("# Product names."), "{text}");
        let list = parse(&text).unwrap();
        assert_eq!(list.accept, ["Kubernetes", "hopefully"]);
        assert_eq!(list.issue.len(), 2);

        edit(&ctx, &scope, |s| {
            assert!(s.remove_accept("kubernetes"));
            assert!(!s.remove_accept("absent"));
            assert_eq!(s.retain_issues(|e| e.file != "~/a.md"), 1);
            Ok(())
        })
        .unwrap();
        let list = parse(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(list.accept, ["hopefully"]);
        assert_eq!(list.issue, [entry("~/b.md", "leverage")]);

        assert_eq!(edit(&ctx, &scope, |s| Ok(s.clear())).unwrap(), 2);
        assert!(
            parse(&fs::read_to_string(&path).unwrap())
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn inline_issue_arrays_are_kept_when_adding() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = ctx(dir.path());
        let path = path(&ctx, &ListScope::Global).unwrap();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let inline = [
            (
                "schema = 1\nissue = [{ file = \"a.md\", match = \"x\" }]\n",
                2,
            ),
            ("schema = 1\nissue = []\n", 1),
        ];
        for (text, after) in inline {
            fs::write(&path, text).unwrap();
            edit(&ctx, &ListScope::Global, |s| {
                Ok(s.add_issue(entry("b.md", "y")))
            })
            .unwrap();
            let list = parse(&fs::read_to_string(&path).unwrap()).unwrap();
            assert_eq!(list.issue.len(), after, "{text}");
        }
    }

    #[test]
    fn a_file_that_doesnt_parse_is_never_written() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = ctx(dir.path());
        let path = path(&ctx, &ListScope::Global).unwrap();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let broken = "schema = 1\naccept = [\"a\"\n";
        fs::write(&path, broken).unwrap();
        let e = edit(&ctx, &ListScope::Global, |s| Ok(s.add_accept("b"))).unwrap_err();
        assert!(e.to_string().contains("ignores.toml"), "{e}");
        assert_eq!(fs::read_to_string(&path).unwrap(), broken);
    }

    #[test]
    fn files_are_named_relative_to_the_project_root() {
        let ctx = Ctx::new("/x/proj/docs".into(), Some("/home/me".into()), None, false);
        let project = ListScope::Project("/x/proj".into());
        assert_eq!(
            file_for(&ctx, &project, Path::new("/x/proj/docs/a.md")),
            "docs/a.md"
        );
        // A sibling whose name starts the same isn't inside the project.
        assert_eq!(
            file_for(&ctx, &project, Path::new("/x/proj-old/a.md")),
            "/x/proj-old/a.md"
        );
        assert_eq!(
            file_for(&ctx, &ListScope::Global, Path::new("/home/me/n.md")),
            "~/n.md"
        );
        assert_eq!(
            resolve(&ctx, &project, "docs/a.md"),
            Path::new("/x/proj/docs/a.md")
        );
        assert_eq!(
            resolve(&ctx, &ListScope::Global, "a.md"),
            Path::new("/x/proj/docs/a.md")
        );
        assert_eq!(
            resolve(&ctx, &ListScope::Global, "~/n.md"),
            Path::new("/home/me/n.md")
        );
    }

    #[test]
    fn issue_numbers() {
        assert_eq!(parse_numbers("3,1,2-4").unwrap(), [1..=4]);
        assert_eq!(parse_numbers(" 5 ").unwrap(), [5..=5]);
        assert_eq!(parse_numbers("7,1-2,5").unwrap(), [1..=2, 5..=5, 7..=7]);
        assert_eq!(
            parse_numbers("2-99999999999").unwrap(),
            [2..=99_999_999_999]
        );
        assert!(parse_numbers("0").is_err());
        assert!(parse_numbers("x").is_err());
        assert!(parse_numbers("4-2").is_err());
        assert!(parse_numbers(",").is_err());
    }
}
