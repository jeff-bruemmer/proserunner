//! Config files: one schema for every layer, merged in a fixed order.
//!
//! Layers, lowest first: built-in defaults, the user config
//! (`~/.config/proserunner/config.toml`), the project config
//! (`.proserunner/config.toml`), then `--config FILE`. Tables merge key by
//! key. A later layer's scalars and lists replace earlier ones, except
//! `accept`, `mask` and `exclude`, which append; `[formats]` rows and
//! `[[overrides]]` append too and apply in order. `inherit = false` drops
//! every layer below the one that says it, unread.
//!
//! Paths are relative to the layer's base: the project root for a project
//! config, otherwise the directory that holds the file. `~/` is the home
//! directory. Path patterns follow gitignore: one with no slash matches at
//! any depth below the project root (or the working directory, outside a
//! project), never a directory above it, and one with a slash (or a
//! leading `/`) is anchored to the base.
//!
//! Every value remembers the file it came from, for `proserunner config`.

#[cfg(test)]
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt::{self, Display, Formatter, Write as _};
use std::fs;
use std::path::{Path, PathBuf};

use globset::{Glob, GlobBuilder, GlobSet, GlobSetBuilder};
use regex::Regex;
#[cfg(test)]
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::Deserialize;
use serde::de::value::MapAccessDeserializer;
use serde::de::{self, Deserializer, MapAccess, Visitor};

use crate::ctx::{self, Ctx, ListScope, PROJECT_DIR};
use crate::english;
use crate::error::{Error, Result};
use crate::format::Output;
use crate::prose;
use crate::rules::{DEFAULT_SOURCE, Level};
use crate::scan::Format;
use crate::settings;
use crate::typo;

/// The only config format this version reads.
pub const SCHEMA: u32 = 1;

/// A config file's name, in the user config directory and in
/// `.proserunner/`.
pub const FILE: &str = "config.toml";

/// The origin of a value no layer set.
const BUILT_IN: &str = "built-in default";

// ---------------------------------------------------------------------
// The file format.

/// A proserunner config file: the user's
/// (`~/.config/proserunner/config.toml`), a project's
/// (`.proserunner/config.toml`), or one given with `--config`. Each
/// layers over the one before; `proserunner config` shows the result.
#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[cfg_attr(test, schemars(title = "proserunner config"))]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[non_exhaustive]
pub struct ConfigFile {
    /// The format version: 1.
    #[cfg_attr(test, schemars(extend("const" = 1)))]
    pub schema: u32,
    /// `false` leaves out the configs below this one, and their ignore
    /// lists.
    pub inherit: Option<bool>,
    /// Where rules come from: `"default"` for the built-in rules, or
    /// directories of rule files. Replaces the list below.
    pub rules: Option<Vec<SourceSpec>>,
    /// The least serious level that makes a run exit 1.
    pub fail_on: Option<Level>,
    /// Matches no rule reports, compared whole and in any case. Adds to
    /// the list below.
    #[serde(default)]
    pub accept: Vec<String>,
    /// Regular expressions for text no rule should see, such as template
    /// syntax. Adds to the list below.
    #[serde(default)]
    pub mask: Vec<String>,
    /// Defaults for `proserunner check`.
    #[serde(default)]
    pub check: CheckTable,
    /// The format of files a path pattern matches, such as
    /// `"*.qmd" = "markdown"`, for files whose extension says nothing or
    /// the wrong thing. Adds to the rows below; the last row that matches
    /// a file wins.
    #[serde(default, deserialize_with = "rows")]
    #[cfg_attr(test, schemars(schema_with = "formats_schema", extend("default" = {})))]
    pub formats: Vec<FormatPattern>,
    /// Settings for one rule, by id: `[rule."default/hedging"]`.
    #[serde(default)]
    pub rule: BTreeMap<String, RuleTable>,
    /// Settings for the files that match `paths`. They apply in order, so
    /// a later one wins.
    #[serde(default)]
    pub overrides: Vec<OverrideTable>,
}

/// Defaults for `check`'s options.
#[derive(Debug, Default, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[non_exhaustive]
pub struct CheckTable {
    /// Files to skip, as gitignore patterns. Adds to the list below.
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Check quoted text too.
    pub quoted_text: Option<bool>,
    /// Check code blocks too.
    pub code_blocks: Option<bool>,
    /// How issues are printed when `--output` doesn't say.
    pub output: Option<Output>,
}

/// Settings for one rule.
#[derive(Debug, Default, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct RuleTable {
    /// Its level, or `off`.
    pub level: Option<Setting>,
    /// Its matches not to report, compared whole and in any case.
    #[serde(default)]
    pub accept: Vec<String>,
}

/// One `[[overrides]]` entry: settings for some files only.
#[derive(Debug, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct OverrideTable {
    /// Gitignore patterns for the files this applies to.
    pub paths: Vec<String>,
    /// Matches no rule reports in these files.
    #[serde(default)]
    pub accept: Vec<String>,
    /// Rule settings for these files.
    #[serde(default)]
    pub rule: BTreeMap<String, RuleTable>,
}

/// A rule's level, or `off`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
#[expect(missing_docs, reason = "variant docs would split the schema's enum")]
pub enum Setting {
    Off,
    Hint,
    Warning,
    Error,
}

impl Setting {
    /// The level it sets, or `None` for `off`.
    pub fn level(self) -> Option<Level> {
        match self {
            Setting::Off => None,
            Setting::Hint => Some(Level::Hint),
            Setting::Warning => Some(Level::Warning),
            Setting::Error => Some(Level::Error),
        }
    }

    /// As written in config.
    pub fn as_str(self) -> &'static str {
        match self {
            Setting::Off => "off",
            Setting::Hint => "hint",
            Setting::Warning => "warning",
            Setting::Error => "error",
        }
    }
}

impl Display for Setting {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.pad(self.as_str())
    }
}

/// One `rules` entry: `"default"`, a directory, or a table with `path`
/// and `prefix` to choose the id prefix (by default the directory name).
#[derive(Debug, Clone, PartialEq)]
#[expect(clippy::exhaustive_enums, reason = "closed: built-in rules or a path")]
pub enum SourceSpec {
    /// `"default"`: the built-in rules.
    Default,
    /// A directory of rule files.
    Dir {
        /// As written: relative to the layer's base, or `~/…`.
        path: String,
        /// The first part of its rules' ids, if given.
        prefix: Option<String>,
    },
}

/// A rule directory with an id prefix of its own.
#[derive(Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
struct SourceTable {
    /// The directory.
    path: String,
    /// The first part of its rules' ids. By default, the directory's name.
    prefix: Option<String>,
}

#[cfg(test)]
impl JsonSchema for SourceSpec {
    fn schema_name() -> Cow<'static, str> {
        "Source".into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        schemars::json_schema!({
            "description": "`\"default\"` for the built-in rules, a directory of rule files, or `{ path, prefix }`. A project config's paths are relative to the project root; any other config's, to its directory.",
            "anyOf": [{ "type": "string" }, generator.subschema_for::<SourceTable>()]
        })
    }
}

// Hand-written for the same reason as `rules::Entry`: an untagged
// enum would hide which key was wrong.
impl<'de> Deserialize<'de> for SourceSpec {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct SourceVisitor;

        impl<'de> Visitor<'de> for SourceVisitor {
            type Value = SourceSpec;

            fn expecting(&self, f: &mut Formatter) -> fmt::Result {
                f.write_str("\"default\", a directory, or a table with `path`")
            }

            fn visit_str<E: de::Error>(self, s: &str) -> Result<SourceSpec, E> {
                Ok(if s == DEFAULT_SOURCE {
                    SourceSpec::Default
                } else {
                    SourceSpec::Dir {
                        path: s.to_owned(),
                        prefix: None,
                    }
                })
            }

            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<SourceSpec, A::Error> {
                let t = SourceTable::deserialize(MapAccessDeserializer::new(map))?;
                Ok(SourceSpec::Dir {
                    path: t.path,
                    prefix: t.prefix,
                })
            }
        }

        deserializer.deserialize_any(SourceVisitor)
    }
}

/// A `[formats]` row as written.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct FormatPattern {
    /// Which files: a path pattern, such as `"*.mdx"`.
    pub pattern: String,
    /// What they are: a format's name.
    pub format: String,
}

/// A table's keys and values in file order, which `[formats]` needs
/// because a later row wins.
fn rows<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<FormatPattern>, D::Error> {
    struct RowsVisitor;

    impl<'de> Visitor<'de> for RowsVisitor {
        type Value = Vec<FormatPattern>;

        fn expecting(&self, f: &mut Formatter) -> fmt::Result {
            f.write_str("a table of path patterns and format names")
        }

        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
            let mut rows = Vec::new();
            while let Some((pattern, format)) = map.next_entry()? {
                rows.push(FormatPattern { pattern, format });
            }
            Ok(rows)
        }
    }

    deserializer.deserialize_map(RowsVisitor)
}

#[cfg(test)]
fn formats_schema(_: &mut SchemaGenerator) -> Schema {
    let names: Vec<&str> = Format::all().map(Format::name).collect();
    schemars::json_schema!({
        "type": "object",
        "additionalProperties": { "enum": names }
    })
}

/// Parses a config file's text and checks its format version. The error
/// is a message for [`Error::config`].
pub fn parse(text: &str) -> Result<ConfigFile, String> {
    let file: ConfigFile = settings::parse(text)?;
    settings::check_schema(file.schema, SCHEMA)?;
    Ok(file)
}

// ---------------------------------------------------------------------
// The merged result.

/// A merged value and the file that set it.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Sourced<T> {
    /// The setting, as merged.
    pub value: T,
    /// The file that set it, as shown to people, or "built-in default".
    pub origin: String,
}

impl<T> Sourced<T> {
    fn new(value: T, origin: &str) -> Self {
        Sourced {
            value,
            origin: origin.to_owned(),
        }
    }

    fn built_in(value: T) -> Self {
        Sourced::new(value, BUILT_IN)
    }
}

/// A resolved `rules` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
#[expect(clippy::exhaustive_enums, reason = "closed: built-in rules or a path")]
pub enum RuleDir {
    /// The built-in rules.
    Builtin,
    /// A directory of rule files.
    Dir {
        /// Absolute.
        path: PathBuf,
        /// The first part of its rules' ids.
        prefix: String,
    },
}

impl RuleDir {
    /// The first part of its rules' ids: `default` for the built-in rules.
    pub fn prefix(&self) -> &str {
        match self {
            RuleDir::Builtin => DEFAULT_SOURCE,
            RuleDir::Dir { prefix, .. } => prefix,
        }
    }
}

/// Path patterns as written, and compiled (each against its own layer's
/// base) to match absolute paths.
#[derive(Debug, Clone, Default)]
pub struct Paths {
    written: Vec<Sourced<String>>,
    globs: Vec<Glob>,
    /// Per glob: for a pattern without a `/`, the directory it matches
    /// below. Its glob is relative, and matches the part of a path that
    /// [`ctx::below`] gives.
    below: Vec<Option<PathBuf>>,
    set: GlobSet,
}

impl Paths {
    /// The patterns as written, with where each was.
    pub fn written(&self) -> &[Sourced<String>] {
        &self.written
    }

    /// True when there are no patterns.
    pub fn is_empty(&self) -> bool {
        self.globs.is_empty()
    }

    /// Whether a pattern matches `path` (absolute).
    pub fn is_match(&self, path: &Path) -> bool {
        if self.is_empty() {
            return false;
        }
        let hit = |candidate: &Path, root: Option<&Path>| {
            self.set
                .matches(candidate)
                .into_iter()
                .any(|i| self.below.get(i).and_then(Option::as_deref) == root)
        };
        if hit(path, None) {
            return true;
        }
        let mut roots: Vec<&Path> = self.below.iter().flatten().map(PathBuf::as_path).collect();
        roots.dedup();
        roots
            .into_iter()
            .any(|root| hit(ctx::below(path, root), Some(root)))
    }

    fn extend(&mut self, more: Paths) -> Result<(), String> {
        self.written.extend(more.written);
        self.globs.extend(more.globs);
        self.below.extend(more.below);
        self.set = build_set(&self.globs)?;
        Ok(())
    }
}

/// A rule's `[rule."…"]` tables, merged.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct RuleSettings {
    /// Its level, or `off`; `None` when no layer sets one.
    pub level: Option<Sourced<Setting>>,
    /// Its matches not to report, normalized like matches.
    pub accept: Vec<Sourced<String>>,
}

/// An `[[overrides]]` entry, with its paths compiled.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Override {
    /// The file it's in, as shown to people.
    pub origin: String,
    /// The files it applies to.
    pub paths: Paths,
    /// Matches no rule reports in these files, normalized like matches.
    pub accept: Vec<String>,
    /// Rule settings for these files, by id.
    pub rule: BTreeMap<String, RuleSettings>,
}

/// A `[formats]` row: the files one pattern matches, and their format.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct FormatRow {
    /// The row's pattern.
    pub paths: Paths,
    /// The format its files are read as.
    pub format: Format,
}

/// Which scopes' ignore lists apply, after `inherit = false`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Inherited {
    /// Whether the user's ignore list applies.
    pub global: bool,
    /// Whether the project's ignore list applies.
    pub project: bool,
}

impl Inherited {
    /// Whether `scope`'s ignore list applies.
    pub fn includes(self, scope: &ListScope) -> bool {
        match scope {
            ListScope::Global => self.global,
            ListScope::Project(_) => self.project,
        }
    }
}

/// Every layer merged over the built-in defaults: what a run uses, each
/// setting with the file that set it.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Config {
    /// The files merged, lowest first, as shown to people.
    pub files: Vec<String>,
    /// Which ignore lists apply.
    pub inherited: Inherited,
    /// Where rules come from.
    pub rules: Sourced<Vec<RuleDir>>,
    /// The least serious level that makes a run exit 1.
    pub fail_on: Sourced<Level>,
    /// Matches no rule reports, normalized like matches.
    pub accept: Vec<Sourced<String>>,
    /// Regexes for text no rule sees.
    pub mask: Vec<Sourced<String>>,
    /// Files not to check.
    pub exclude: Paths,
    /// Whether quoted text is checked.
    pub quoted_text: Sourced<bool>,
    /// Whether code blocks are checked.
    pub code_blocks: Sourced<bool>,
    /// How issues are printed when `--output` doesn't say.
    pub output: Sourced<Output>,
    /// In order: a later row wins.
    pub formats: Vec<FormatRow>,
    /// Settings for each rule, by id.
    pub rule: BTreeMap<String, RuleSettings>,
    /// In order: a later one wins.
    pub overrides: Vec<Override>,
}

impl Config {
    /// The format of the file at `path` (absolute): the last `[formats]`
    /// row that matches it, else its extension's. `None` when proserunner
    /// doesn't read it.
    pub fn format_of(&self, path: &Path) -> Option<Format> {
        self.formats
            .iter()
            .rev()
            .find(|row| row.paths.is_match(path))
            .map(|row| row.format)
            .or_else(|| Format::of(path))
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            files: Vec::new(),
            inherited: Inherited {
                global: true,
                project: true,
            },
            rules: Sourced::built_in(vec![RuleDir::Builtin]),
            fail_on: Sourced::built_in(Level::Warning),
            accept: Vec::new(),
            mask: Vec::new(),
            exclude: Paths::default(),
            quoted_text: Sourced::built_in(false),
            code_blocks: Sourced::built_in(false),
            output: Sourced::built_in(Output::default()),
            formats: Vec::new(),
            rule: BTreeMap::new(),
            overrides: Vec::new(),
        }
    }
}

/// Which config a layer is, which decides what `inherit = false` drops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum LayerKind {
    /// The user config, in the config directory.
    User,
    /// The project config, in `.proserunner/`.
    Project,
    /// `--config FILE`.
    Explicit,
}

/// One config file, read.
#[derive(Debug)]
#[non_exhaustive]
pub struct Layer {
    /// Which config it is.
    pub kind: LayerKind,
    /// Relative paths in the file resolve against this.
    pub base: PathBuf,
    /// Where its path patterns without a `/` match: at any depth below
    /// this directory, the project root, or below the working directory
    /// when `None`.
    pub within: Option<PathBuf>,
    /// How its path is shown.
    pub origin: String,
    /// What it says.
    pub file: ConfigFile,
}

/// Reads and merges every layer that applies: the user config, the
/// project config when `project` is a project root, and `explicit`
/// (`--config`), which must exist. Each layer's path patterns without a
/// `/` match below `project`, or the working directory when it's `None`.
pub fn load(ctx: &Ctx, project: Option<&Path>, explicit: Option<&Path>) -> Result<Config> {
    let read = |kind, path: &Path, base: &Path| read_layer(ctx, kind, path, base, project);
    // Highest first, so a layer with `inherit = false` keeps the ones below
    // it from being read: a broken or newer user config can't stop a
    // project that left it out.
    let mut layers = Vec::new();
    let inherits = |layers: &[Layer]| layers.last().is_none_or(|l| l.file.inherit != Some(false));
    if let Some(given) = explicit {
        let path = ctx.absolute(given);
        if !path.is_file() {
            return Err(Error::NotFound(given.to_path_buf()));
        }
        let base = path.parent().unwrap_or(Path::new("/")).to_path_buf();
        layers.push(read(LayerKind::Explicit, &path, &base)?);
    }
    if let Some(root) = project.filter(|_| inherits(&layers)) {
        let path = root.join(PROJECT_DIR).join(FILE);
        if path.is_file() {
            layers.push(read(LayerKind::Project, &path, root)?);
        }
    }
    if let Some(dir) = ctx.config_dir.as_deref().filter(|_| inherits(&layers)) {
        let path = dir.join(FILE);
        if path.is_file() {
            layers.push(read(LayerKind::User, &path, dir)?);
        }
    }
    layers.reverse();
    merge(ctx, layers)
}

fn read_layer(
    ctx: &Ctx,
    kind: LayerKind,
    path: &Path,
    base: &Path,
    within: Option<&Path>,
) -> Result<Layer> {
    let origin = ctx.display(path);
    let text = fs::read_to_string(path).map_err(|e| Error::io(path, e))?;
    let file = parse(&text).map_err(|m| Error::config(&origin, m))?;
    Ok(Layer {
        kind,
        base: base.to_path_buf(),
        within: within.map(Path::to_path_buf),
        origin,
        file,
    })
}

/// Merges layers, lowest first, over the built-in defaults.
pub fn merge(ctx: &Ctx, layers: Vec<Layer>) -> Result<Config> {
    let mut config = Config::default();
    for layer in layers {
        if layer.file.inherit == Some(false) {
            let mut inherited = config.inherited;
            match layer.kind {
                LayerKind::User => {}
                LayerKind::Project => inherited.global = false,
                LayerKind::Explicit => {
                    inherited.global = false;
                    inherited.project = false;
                }
            }
            config = Config {
                inherited,
                ..Config::default()
            };
        }
        apply(ctx, &mut config, layer).map_err(|(origin, m)| Error::config(origin, m))?;
    }
    Ok(config)
}

type Failure = (String, String);

fn apply(ctx: &Ctx, config: &mut Config, layer: Layer) -> Result<(), Failure> {
    let origin = layer.origin.as_str();
    let fail = |m: String| (origin.to_owned(), m);
    let f = layer.file;
    config.files.push(origin.to_owned());

    if let Some(specs) = f.rules {
        let dirs: Vec<RuleDir> = specs
            .iter()
            .map(|s| rule_dir(ctx, &layer.base, s))
            .collect::<Result<_, _>>()
            .map_err(fail)?;
        config.rules = Sourced::new(dirs, origin);
    }
    if let Some(level) = f.fail_on {
        config.fail_on = Sourced::new(level, origin);
    }
    config.accept.extend(
        accepted(&f.accept)
            .map_err(fail)?
            .map(|a| Sourced::new(a, origin)),
    );
    for m in f.mask {
        Regex::new(&m).map_err(|e| fail(format!("invalid mask `{m}`: {e}")))?;
        config.mask.push(Sourced::new(m, origin));
    }
    let exclude: Vec<Sourced<String>> = f
        .check
        .exclude
        .into_iter()
        .map(|p| Sourced::new(p, origin))
        .collect();
    let exclude = paths(ctx, &layer.base, layer.within.as_deref(), exclude).map_err(fail)?;
    config.exclude.extend(exclude).map_err(fail)?;
    if let Some(v) = f.check.quoted_text {
        config.quoted_text = Sourced::new(v, origin);
    }
    if let Some(v) = f.check.code_blocks {
        config.code_blocks = Sourced::new(v, origin);
    }
    if let Some(v) = f.check.output {
        config.output = Sourced::new(v, origin);
    }
    for FormatPattern {
        pattern,
        format: name,
    } in f.formats
    {
        let format = Format::named(&name).ok_or_else(|| fail(unknown_format(&pattern, &name)))?;
        let written = vec![Sourced::new(pattern, origin)];
        let paths = paths(ctx, &layer.base, layer.within.as_deref(), written).map_err(fail)?;
        config.formats.push(FormatRow { paths, format });
    }
    merge_rules(&mut config.rule, f.rule, origin).map_err(fail)?;

    for o in f.overrides {
        if o.paths.is_empty() {
            return Err(fail("an [[overrides]] entry needs `paths`".into()));
        }
        let written: Vec<Sourced<String>> = o
            .paths
            .into_iter()
            .map(|p| Sourced::new(p, origin))
            .collect();
        let mut rule = BTreeMap::new();
        merge_rules(&mut rule, o.rule, origin).map_err(fail)?;
        config.overrides.push(Override {
            origin: origin.to_owned(),
            paths: paths(ctx, &layer.base, layer.within.as_deref(), written).map_err(fail)?,
            accept: accepted(&o.accept).map_err(fail)?.collect(),
            rule,
        });
    }
    Ok(())
}

fn merge_rules(
    into: &mut BTreeMap<String, RuleSettings>,
    tables: BTreeMap<String, RuleTable>,
    origin: &str,
) -> Result<(), String> {
    for (id, table) in tables {
        if !id.contains('/') {
            return Err(format!(
                "[rule.\"{id}\"]: rule ids look like `source/name`, such as `default/hedging`"
            ));
        }
        let settings = into.entry(id).or_default();
        if let Some(level) = table.level {
            settings.level = Some(Sourced::new(level, origin));
        }
        settings
            .accept
            .extend(accepted(&table.accept)?.map(|a| Sourced::new(a, origin)));
    }
    Ok(())
}

/// Why a `[formats]` row can't use `name`, with the format it most likely
/// means: the one with that extension, or the closest name. Otherwise,
/// every name.
fn unknown_format(pattern: &str, name: &str) -> String {
    let names: Vec<&str> = Format::all().map(Format::name).collect();
    let by_extension = Format::of(Path::new(&format!("x.{name}"))).map(Format::name);
    let hint = match by_extension.or_else(|| typo::closest(name, names.iter().copied())) {
        Some(c) => format!("did you mean `{c}`?"),
        None => {
            let quoted: Vec<String> = names.iter().map(|n| format!("`{n}`")).collect();
            format!("the formats are {}", english::and_list(&quoted))
        }
    };
    format!(
        "[formats] {}: no format is named `{name}` ({hint})",
        settings::quote(pattern)
    )
}

/// `accept` entries, normalized like matches. Empty ones are an error.
fn accepted(list: &[String]) -> Result<impl Iterator<Item = String>, String> {
    let normalized: Vec<String> = list.iter().map(|a| prose::normalize(a)).collect();
    if normalized.iter().any(String::is_empty) {
        return Err("`accept` entries can't be empty".into());
    }
    Ok(normalized.into_iter())
}

fn rule_dir(ctx: &Ctx, base: &Path, spec: &SourceSpec) -> Result<RuleDir, String> {
    let (path, prefix) = match spec {
        SourceSpec::Default => return Ok(RuleDir::Builtin),
        SourceSpec::Dir { path, prefix } => (path, prefix),
    };
    let resolved = ctx.resolve(base, path);
    let prefix = match prefix {
        Some(p) => p.clone(),
        None => resolved
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
    };
    if prefix.is_empty() || prefix.contains('/') {
        return Err(format!(
            "`{path}`: a rule prefix can't be empty or contain `/`; set one with {{ path = \"{path}\", prefix = \"…\" }}"
        ));
    }
    if prefix == DEFAULT_SOURCE {
        return Err(format!(
            "`{path}`: the prefix `{DEFAULT_SOURCE}` belongs to the built-in rules; set another with {{ path = \"{path}\", prefix = \"…\" }}"
        ));
    }
    Ok(RuleDir::Dir {
        path: resolved,
        prefix,
    })
}

fn paths(
    ctx: &Ctx,
    base: &Path,
    within: Option<&Path>,
    written: Vec<Sourced<String>>,
) -> Result<Paths, String> {
    let root = within.unwrap_or(&ctx.cwd);
    let mut compiled = Vec::new();
    let mut below = Vec::new();
    for p in &written {
        for (glob, relative) in globs(ctx, base, &p.value)? {
            compiled.push(
                GlobBuilder::new(&glob)
                    .literal_separator(true)
                    .build()
                    .map_err(|e| format!("`{}`: {e}", p.value))?,
            );
            below.push(relative.then(|| root.to_path_buf()));
        }
    }
    Ok(Paths {
        set: build_set(&compiled)?,
        written,
        globs: compiled,
        below,
    })
}

fn build_set(globs: &[Glob]) -> Result<GlobSet, String> {
    let mut builder = GlobSetBuilder::new();
    for g in globs {
        builder.add(g.clone());
    }
    builder.build().map_err(|e| e.to_string())
}

/// The globs a gitignore-style pattern stands for, each with whether it's
/// relative. A pattern also matches everything under a directory it
/// matches. One with a `/` is absolute. One without is relative: it
/// matches at any depth below a root (see [`Paths::below`]), as a
/// `.gitignore`'s patterns match only below its directory, never a
/// directory a whole run is inside.
fn globs(ctx: &Ctx, base: &Path, pattern: &str) -> Result<Vec<(String, bool)>, String> {
    let p = pattern.trim();
    if p.is_empty() {
        return Err("path patterns can't be empty".into());
    }
    if p.starts_with('!') {
        return Err(format!("`{p}`: negated patterns aren't supported"));
    }
    let (p, dir_only) = match p.strip_suffix('/') {
        Some(stripped) => (stripped, true),
        None => (p, false),
    };
    let escaped = |dir: &Path| {
        let text = globset::escape(&dir.display().to_string());
        text.trim_end_matches('/').to_owned()
    };
    let anchored = if let Some(rest) = p.strip_prefix("~/") {
        let home = ctx
            .home
            .as_deref()
            .ok_or_else(|| format!("`{pattern}`: `~/` needs HOME to be set"))?;
        vec![(format!("{}/{rest}", escaped(home)), false)]
    } else if p.contains('/') {
        // Leading `../` climbs from the base, since matching compares
        // paths with `..` already resolved.
        let mut base = base.to_path_buf();
        let mut rest = p.trim_start_matches("./").trim_start_matches('/');
        while let Some(up) = rest.strip_prefix("../") {
            base.pop();
            rest = up;
        }
        vec![(format!("{}/{rest}", escaped(&base)), false)]
    } else {
        vec![(format!("**/{p}"), true)]
    };
    let mut out = Vec::new();
    for (glob, relative) in anchored {
        if !dir_only {
            out.push((glob.clone(), relative));
        }
        out.push((format!("{glob}/**"), relative));
    }
    Ok(out)
}

// ---------------------------------------------------------------------
// Showing the merged config.

impl Config {
    /// The effective config as TOML, each value commented with the file it
    /// came from.
    #[must_use]
    pub fn render(&self, ctx: &Ctx) -> String {
        let mut out = String::new();
        if self.files.is_empty() {
            out.push_str("# No config files; every value is a built-in default.\n");
        } else {
            let _ = writeln!(out, "# Merged from: {}", self.files.join(", "));
        }
        if !self.inherited.global || !self.inherited.project {
            out.push_str("# inherit = false: lower layers and their ignores are left out.\n");
        }
        out.push_str("schema = 1\n");

        let rules: Vec<String> = self
            .rules
            .value
            .iter()
            .map(|d| match d {
                RuleDir::Builtin => settings::quote(DEFAULT_SOURCE),
                RuleDir::Dir { path, prefix } => format!(
                    "{{ path = {}, prefix = {} }}",
                    settings::quote(&ctx.display(path)),
                    settings::quote(prefix)
                ),
            })
            .collect();
        line(
            &mut out,
            "rules",
            &format!("[{}]", rules.join(", ")),
            &self.rules.origin,
        );
        line(
            &mut out,
            "fail-on",
            &settings::quote(self.fail_on.value.as_str()),
            &self.fail_on.origin,
        );
        list(&mut out, "accept", &self.accept);
        list(&mut out, "mask", &self.mask);

        out.push_str("\n[check]\n");
        list(&mut out, "exclude", &self.exclude.written);
        line(
            &mut out,
            "quoted-text",
            &self.quoted_text.value.to_string(),
            &self.quoted_text.origin,
        );
        line(
            &mut out,
            "code-blocks",
            &self.code_blocks.value.to_string(),
            &self.code_blocks.origin,
        );
        line(
            &mut out,
            "output",
            &settings::quote(self.output.value.as_str()),
            &self.output.origin,
        );

        if !self.formats.is_empty() {
            out.push_str("\n[formats]\n");
        }
        for (i, row) in self.formats.iter().enumerate() {
            // A key can't come twice, so a row whose key comes again is a
            // comment. It never wins when it matches the same files, but
            // a key from another config's base still matches its own.
            let later = &self.formats[i + 1..];
            let (comment, note) = if later.iter().any(|l| l.paths.globs == row.paths.globs) {
                ("# ", ", replaced below")
            } else if later.iter().any(|l| same_patterns(&l.paths, &row.paths)) {
                ("# ", ", still applies; its key repeats below")
            } else {
                ("", "")
            };
            for p in &row.paths.written {
                let _ = writeln!(
                    out,
                    "{comment}{} = {}  # {}{note}",
                    settings::quote(&p.value),
                    settings::quote(row.format.name()),
                    p.origin
                );
            }
        }

        for (id, settings) in &self.rule {
            let _ = writeln!(out, "\n[rule.{}]", settings::quote(id));
            rule_settings(&mut out, "", settings);
        }
        for o in &self.overrides {
            let _ = writeln!(out, "\n[[overrides]]  # {}", o.origin);
            let written: Vec<String> = o
                .paths
                .written
                .iter()
                .map(|p| settings::quote(&p.value))
                .collect();
            let _ = writeln!(out, "paths = [{}]", written.join(", "));
            if !o.accept.is_empty() {
                let accept: Vec<String> = o.accept.iter().map(|a| settings::quote(a)).collect();
                let _ = writeln!(out, "accept = [{}]", accept.join(", "));
            }
            for (id, settings) in &o.rule {
                rule_settings(
                    &mut out,
                    &format!("rule.{}.", settings::quote(id)),
                    settings,
                );
            }
        }
        out
    }
}

fn line(out: &mut String, key: &str, value: &str, origin: &str) {
    let _ = writeln!(out, "{key} = {value}  # {origin}");
}

/// Whether `a` and `b` are written the same, wherever each was.
fn same_patterns(a: &Paths, b: &Paths) -> bool {
    let (a, b) = (&a.written, &b.written);
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.value == y.value)
}

fn list(out: &mut String, key: &str, items: &[Sourced<String>]) {
    if items.is_empty() {
        let _ = writeln!(out, "{key} = []");
        return;
    }
    let _ = writeln!(out, "{key} = [");
    for item in items {
        let _ = writeln!(
            out,
            "    {},  # {}",
            settings::quote(&item.value),
            item.origin
        );
    }
    out.push_str("]\n");
}

fn rule_settings(out: &mut String, prefix: &str, settings: &RuleSettings) {
    if let Some(level) = &settings.level {
        line(
            out,
            &format!("{prefix}level"),
            &settings::quote(level.value.as_str()),
            &level.origin,
        );
    }
    if !settings.accept.is_empty() {
        list(out, &format!("{prefix}accept"), &settings.accept);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> Ctx {
        Ctx::new("/work/proj".into(), Some("/home/me".into()), None, false)
    }

    /// A layer as `load` reads it, in the project `/work/proj`.
    fn layer(kind: LayerKind, base: &str, origin: &str, text: &str) -> Layer {
        Layer {
            kind,
            base: base.into(),
            within: Some("/work/proj".into()),
            origin: origin.into(),
            file: parse(text).unwrap(),
        }
    }

    #[test]
    fn unknown_keys_and_bad_values_are_errors() {
        let e = parse("schema = 1\nfail_on = \"error\"").unwrap_err();
        assert!(e.contains("unknown field `fail_on`"), "{e}");
        let e = parse("schema = 1\n[rule.\"default/x\"]\nlevel = \"loud\"").unwrap_err();
        assert!(e.contains("loud"), "{e}");
        let e = parse("schema = 1\nrules = [{ dir = \"x\" }]").unwrap_err();
        assert!(e.contains("unknown field `dir`"), "{e}");
        let e = parse("fail-on = \"error\"").unwrap_err();
        assert!(e.contains("missing field `schema`"), "{e}");
        let e = parse("schema = 2").unwrap_err();
        assert!(e.contains("schema 2"), "{e}");
    }

    #[test]
    fn errors_have_positions() {
        let e = parse("schema = 1\n\n[check]\nquoted-txt = true\n").unwrap_err();
        assert!(e.contains("line 4"), "{e}");
    }

    #[test]
    fn layers_merge_in_order() {
        let user = layer(
            LayerKind::User,
            "/home/me/.config/proserunner",
            "user.toml",
            r#"schema = 1
            fail-on = "error"
            accept = ["Kubernetes"]
            rules = ["default", "mine"]
            [check]
            quoted-text = true
            [rule."default/hedging"]
            level = "off"
            accept = ["I think"]
            "#,
        );
        let project = layer(
            LayerKind::Project,
            "/work/proj",
            "project.toml",
            r#"schema = 1
            accept = ["Rust"]
            [rule."default/hedging"]
            accept = ["perhaps"]
            [rule."default/jargon"]
            level = "error"
            "#,
        );
        let c = merge(&ctx(), vec![user, project]).unwrap();
        assert_eq!(c.files, ["user.toml", "project.toml"]);
        assert_eq!(c.fail_on.value, Level::Error);
        assert_eq!(c.fail_on.origin, "user.toml");
        let accept: Vec<_> = c.accept.iter().map(|a| a.value.as_str()).collect();
        assert_eq!(accept, ["Kubernetes", "Rust"]);
        assert_eq!(
            c.rules.value,
            [
                RuleDir::Builtin,
                RuleDir::Dir {
                    path: "/home/me/.config/proserunner/mine".into(),
                    prefix: "mine".into()
                }
            ]
        );
        assert!(c.quoted_text.value);
        let hedging = &c.rule["default/hedging"];
        assert_eq!(hedging.level.as_ref().unwrap().value, Setting::Off);
        assert_eq!(hedging.accept.len(), 2);
        assert_eq!(
            c.rule["default/jargon"].level.as_ref().unwrap().origin,
            "project.toml"
        );
    }

    #[test]
    fn later_lists_replace_rules() {
        let user = layer(
            LayerKind::User,
            "/u",
            "u",
            "schema = 1\nrules = [\"default\", \"mine\"]",
        );
        let project = layer(
            LayerKind::Project,
            "/work/proj",
            "p",
            "schema = 1\nrules = [{ path = \".proserunner/rules\", prefix = \"project\" }]",
        );
        let c = merge(&ctx(), vec![user, project]).unwrap();
        assert_eq!(
            c.rules.value,
            [RuleDir::Dir {
                path: "/work/proj/.proserunner/rules".into(),
                prefix: "project".into()
            }]
        );
    }

    #[test]
    fn inherit_false_drops_lower_layers_and_their_ignores() {
        let user = layer(
            LayerKind::User,
            "/u",
            "u",
            "schema = 1\naccept = [\"x\"]\nfail-on = \"error\"",
        );
        let project = layer(LayerKind::Project, "/p", "p", "schema = 1\ninherit = false");
        let c = merge(&ctx(), vec![user, project]).unwrap();
        assert!(c.accept.is_empty());
        assert_eq!(c.fail_on.value, Level::Warning);
        assert_eq!(c.files, ["p"]);
        assert_eq!(
            c.inherited,
            Inherited {
                global: false,
                project: true
            }
        );
    }

    #[test]
    fn bad_prefixes_and_masks_name_the_file() {
        let bad = |text: &str| {
            merge(&ctx(), vec![layer(LayerKind::User, "/u", "u.toml", text)])
                .unwrap_err()
                .to_string()
        };
        assert!(
            bad("schema = 1\nrules = [{ path = \"x\", prefix = \"default\" }]")
                .contains("belongs to the built-in rules")
        );
        assert!(bad("schema = 1\nmask = [\"(\"]").starts_with("u.toml: invalid mask"));
        assert!(bad("schema = 1\n[rule.hedging]\nlevel = \"off\"").contains("source/name"));
        assert!(bad("schema = 1\naccept = [\" \"]").contains("can't be empty"));
    }

    #[test]
    fn excludes_keep_their_own_base() {
        let user = layer(
            LayerKind::User,
            "/u",
            "u",
            "schema = 1\n[check]\nexclude = [\"notes/a.md\"]",
        );
        let project = layer(
            LayerKind::Project,
            "/work/proj",
            "p",
            "schema = 1\n[check]\nexclude = [\"docs/b.md\"]",
        );
        let c = merge(&ctx(), vec![user, project]).unwrap();
        assert!(c.exclude.is_match(Path::new("/u/notes/a.md")));
        assert!(c.exclude.is_match(Path::new("/work/proj/docs/b.md")));
        assert!(!c.exclude.is_match(Path::new("/work/proj/notes/a.md")));
    }

    #[test]
    fn path_patterns_follow_gitignore() {
        let c = ctx();
        let base = Path::new("/work/proj");
        let written = |p: &str| vec![Sourced::built_in(p.to_owned())];
        let set = |p: &str| paths(&c, base, Some(base), written(p)).unwrap();
        let any_depth = set("*.draft.md");
        assert!(any_depth.is_match(Path::new("/work/proj/a.draft.md")));
        assert!(any_depth.is_match(Path::new("/work/proj/deep/b.draft.md")));
        assert!(any_depth.is_match(Path::new("/elsewhere/deep/b.draft.md")));
        assert!(!any_depth.is_match(Path::new("/work/proj/a.md")));

        let dir = set("drafts/");
        assert!(dir.is_match(Path::new("/work/proj/docs/drafts/a.md")));
        assert!(!dir.is_match(Path::new("/work/proj/drafts.md")));
        // Only directories below the project root count, or for a file
        // outside it, those below the directory they share.
        let above = set("work/");
        assert!(!above.is_match(Path::new("/work/proj/docs/a.md")));
        assert!(above.is_match(Path::new("/work/proj/work/a.md")));
        assert!(!above.is_match(Path::new("/work/other/a.md")));
        assert!(above.is_match(Path::new("/elsewhere/work/a.md")));
        // With no project, the working directory is the root.
        let drafts = Ctx::new("/x/drafts/novel".into(), None, None, false);
        let no_project = paths(&drafts, base, None, written("drafts")).unwrap();
        assert!(!no_project.is_match(Path::new("/x/drafts/novel/a.md")));
        assert!(!no_project.is_match(Path::new("/x/drafts/other/a.md")));
        assert!(no_project.is_match(Path::new("/x/drafts/novel/drafts/a.md")));

        let anchored = set("docs/legal");
        assert!(anchored.is_match(Path::new("/work/proj/docs/legal/a.md")));
        assert!(!anchored.is_match(Path::new("/work/proj/sub/docs/legal/a.md")));

        let star = set("/docs/*.md");
        assert!(star.is_match(Path::new("/work/proj/docs/a.md")));
        assert!(!star.is_match(Path::new("/work/proj/docs/sub/a.md")));

        assert!(set("~/notes/**").is_match(Path::new("/home/me/notes/x/a.md")));
        assert!(set("../shared/*.md").is_match(Path::new("/work/shared/a.md")));
        assert!(!set("../shared/*.md").is_match(Path::new("/work/proj/shared/a.md")));
        assert!(!Paths::default().is_match(Path::new("/a.md")));
    }

    #[test]
    fn render_shows_where_values_came_from() {
        let user = layer(
            LayerKind::User,
            "/u",
            "~/.config/proserunner/config.toml",
            "schema = 1\naccept = [\"Kubernetes\"]\n[rule.\"default/hedging\"]\nlevel = \"off\"\n[[overrides]]\npaths = [\"legal/\"]\nrule.\"default/jargon\".level = \"error\"",
        );
        let text = merge(&ctx(), vec![user]).unwrap().render(&ctx());
        assert!(
            text.contains("fail-on = \"warning\"  # built-in default"),
            "{text}"
        );
        assert!(
            text.contains("    \"Kubernetes\",  # ~/.config/proserunner/config.toml"),
            "{text}"
        );
        assert!(
            text.contains("[rule.\"default/hedging\"]\nlevel = \"off\"  # ~/"),
            "{text}"
        );
        assert!(
            text.contains("rule.\"default/jargon\".level = \"error\""),
            "{text}"
        );
        // What it prints is a config file this version reads.
        parse(&text).unwrap();
    }

    #[test]
    fn formats_rows_apply_in_order_over_extensions() {
        let user = layer(
            LayerKind::User,
            "/u",
            "u",
            "schema = 1\n[formats]\n\"*.mdx\" = \"org\"\n\"*.rmd\" = \"markdown\"",
        );
        // `*.md` sorts before `legacy/`, so a sorted table would let
        // `legacy/` win.
        let project = layer(
            LayerKind::Project,
            "/work/proj",
            "p",
            "schema = 1\n[formats]\n\"*.mdx\" = \"markdown\"\n\"legacy/\" = \"text\"\n\"*.md\" = \"markdown\"",
        );
        let c = merge(&ctx(), vec![user, project]).unwrap();
        let of = |p: &str| c.format_of(Path::new(p));
        assert_eq!(of("/work/proj/a.mdx"), Some(Format::Markdown));
        assert_eq!(of("/elsewhere/b.rmd"), Some(Format::Markdown));
        assert_eq!(of("/work/proj/legacy/a.org"), Some(Format::Text));
        assert_eq!(of("/work/proj/legacy/a.md"), Some(Format::Markdown));
        assert_eq!(of("/work/proj/a.txt"), Some(Format::Text));
        assert_eq!(of("/work/proj/a.org"), Some(Format::Org));
        assert_eq!(of("/work/proj/main.rs"), None);
    }

    #[test]
    fn unknown_format_names_get_a_hint() {
        let bad = |name: &str| {
            let text = format!("schema = 1\n[formats]\n\"*.x\" = \"{name}\"");
            merge(&ctx(), vec![layer(LayerKind::User, "/u", "u.toml", &text)])
                .unwrap_err()
                .to_string()
        };
        assert_eq!(
            bad("markdwn"),
            "u.toml: [formats] \"*.x\": no format is named `markdwn` (did you mean `markdown`?)"
        );
        assert!(bad("txt").ends_with("(did you mean `text`?)"));
        assert!(bad("md").ends_with("(did you mean `markdown`?)"));
        assert!(bad("ascidoc").ends_with("(did you mean `asciidoc`?)"));
        assert_eq!(
            bad("restructuredtext"),
            "u.toml: [formats] \"*.x\": no format is named `restructuredtext` (the formats are `markdown`, `mdx`, `org`, `text`, `html`, `asciidoc` and `typst`)"
        );
        let e = parse("schema = 1\n[formats]\n\"*.x\" = 1").unwrap_err();
        assert!(e.contains("invalid type"), "{e}");
    }

    #[test]
    fn render_shows_formats_and_replaced_rows() {
        let user = layer(
            LayerKind::User,
            "/u",
            "u.toml",
            "schema = 1\n[formats]\n\"*.mdx\" = \"org\"\n\"docs/*.txt\" = \"org\"",
        );
        let project = layer(
            LayerKind::Project,
            "/work/proj",
            "p.toml",
            "schema = 1\n[formats]\n\"*.mdx\" = \"markdown\"",
        );
        let text = merge(&ctx(), vec![user, project]).unwrap().render(&ctx());
        assert!(
            text.contains(
                "\n[formats]\n\
                 # \"*.mdx\" = \"org\"  # u.toml, replaced below\n\
                 \"docs/*.txt\" = \"org\"  # u.toml\n\
                 \"*.mdx\" = \"markdown\"  # p.toml\n"
            ),
            "{text}"
        );
        assert_eq!(parse(&text).unwrap().formats.len(), 2);

        // The same key from two bases matches different files, so both
        // apply, but only the last can be a key.
        let user = layer(
            LayerKind::User,
            "/u",
            "u.toml",
            "schema = 1\n[formats]\n\"docs/*.txt\" = \"org\"",
        );
        let project = layer(
            LayerKind::Project,
            "/work/proj",
            "p.toml",
            "schema = 1\n[formats]\n\"docs/*.txt\" = \"markdown\"",
        );
        let text = merge(&ctx(), vec![user, project]).unwrap().render(&ctx());
        assert!(
            text.contains(
                "# \"docs/*.txt\" = \"org\"  # u.toml, still applies; its key repeats below\n\
                 \"docs/*.txt\" = \"markdown\"  # p.toml\n"
            ),
            "{text}"
        );
        assert_eq!(parse(&text).unwrap().formats.len(), 1);
        let none = merge(&ctx(), vec![]).unwrap().render(&ctx());
        assert!(!none.contains("[formats]"), "{none}");
    }

    #[test]
    fn quoting() {
        assert_eq!(settings::quote(r"\b"), r"'\b'");
        assert_eq!(settings::quote("it's"), "\"it's\"");
        assert_eq!(settings::quote(r"it's \b"), r#""it's \\b""#);
    }
}
