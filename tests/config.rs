//! Black-box tests of config layers, ignores, scope and migration from
//! 0.8, including regression tests for 0.8's bugs (B1–B7 in the port
//! plan).

#![allow(clippy::unwrap_used)]

mod common;

use std::fs;
use std::process::Stdio;

use common::{CLEAN, DIRTY, Env, files, run};
use serde_json::Value;

/// The rules each issue in a plain run came from, in order.
fn rules_found(env: &Env, args: &[&str]) -> Vec<String> {
    let r = run(env.cmd().args(args).args(["-o", "json"]));
    assert!(r.code < 2, "{}", r.stderr);
    let v: Value = serde_json::from_str(&r.stdout).unwrap();
    v["issues"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["rule"].as_str().unwrap().to_owned())
        .collect()
}

// ---------------------------------------------------------------------
// Config.

#[test]
fn layers_merge_and_config_shows_where_values_came_from() {
    let env = Env::new();
    env.write(
        &format!("{}/config.toml", Env::CONFIG),
        "schema = 1\nfail-on = \"error\"\naccept = [\"synergy\"]\n",
    );
    env.write(
        ".proserunner/config.toml",
        "schema = 1\n[rule.\"default/jargon\"]\nlevel = \"off\"\n",
    );
    env.write("dirty.md", DIRTY);

    assert_eq!(
        rules_found(&env, &["dirty.md"]),
        ["default/needless-variants", "default/compression"]
    );
    // Warnings don't fail the run under fail-on = "error"; the flag wins.
    assert_eq!(run(env.cmd().arg("dirty.md")).code, 0);
    assert_eq!(
        run(env.cmd().args(["dirty.md", "--fail-on", "warning"])).code,
        1
    );

    let r = run(env.cmd().arg("config"));
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(
        r.stdout
            .contains("fail-on = \"error\"  # ~/.config/proserunner/config.toml"),
        "{}",
        r.stdout
    );
    assert!(
        r.stdout
            .contains("[rule.\"default/jargon\"]\nlevel = \"off\"  # .proserunner/config.toml"),
        "{}",
        r.stdout
    );
    assert!(
        r.stdout.contains("output = \"group\"  # built-in default"),
        "{}",
        r.stdout
    );
}

#[test]
fn levels_overrides_and_fail_on() {
    let env = Env::new();
    env.write(
        ".proserunner/config.toml",
        r#"schema = 1
[rule."default/compression"]
level = "hint"

[[overrides]]
paths = ["legal/"]
rule."default/compression".level = "error"
rule."default/jargon".level = "off"
"#,
    );
    env.write("a.md", DIRTY);
    env.write("legal/b.md", DIRTY);
    let r = run(env.cmd().args(["a.md", "legal/b.md", "-o", "json"]));
    let v: Value = serde_json::from_str(&r.stdout).unwrap();
    let levels: Vec<(String, String, String)> = v["issues"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| {
            (
                i["file"].as_str().unwrap().to_owned(),
                i["rule"].as_str().unwrap().to_owned(),
                i["level"].as_str().unwrap().to_owned(),
            )
        })
        .filter(|(_, rule, _)| rule == "default/compression" || rule == "default/jargon")
        .collect();
    let t = |f: &str, r: &str, l: &str| (f.to_owned(), r.to_owned(), l.to_owned());
    assert_eq!(
        levels,
        [
            t("a.md", "default/compression", "hint"),
            t("a.md", "default/jargon", "warning"),
            t("legal/b.md", "default/compression", "error"),
        ]
    );

    env.write("hints.md", "It failed due to the fact that it rained.\n");
    env.write(
        ".proserunner/config.toml",
        "schema = 1\n[rule.\"default/compression\"]\nlevel = \"hint\"\n[rule.\"default/jargon\"]\nlevel = \"off\"\n",
    );
    assert_eq!(
        run(env.cmd().arg("hints.md")).code,
        0,
        "hints don't fail by default"
    );
    assert_eq!(
        run(env.cmd().args(["hints.md", "--fail-on", "hint"])).code,
        1
    );
}

#[test]
fn check_defaults_mask_and_accept() {
    let env = Env::new();
    env.write(
        ".proserunner/config.toml",
        r#"schema = 1
accept = ["SYNERGY"]
mask = ['\{\{<[^>]*>\}\}']

[check]
output = "plain"
quoted-text = true
exclude = ["drafts/"]
"#,
    );
    env.write(
        "docs/a.md",
        "We {{< utilize >}} it. She said \"I think\" and left.\n\nSynergy.\n",
    );
    env.write("docs/drafts/b.md", DIRTY);
    let r = run(env.cmd().arg("docs"));
    // Hedging is a hint, and hints don't fail a run by default.
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(
        r.stdout,
        "docs/a.md:1:34: [1] \"I think\" -> Omit this phrase.\n"
    );
    // A file named directly is checked even when excluded.
    let r = run(env.cmd().arg("docs/drafts/b.md"));
    assert!(
        r.stdout.starts_with("docs/drafts/b.md:1:4:"),
        "{}",
        r.stdout
    );
}

#[test]
fn formats_map_files_to_scanners() {
    let env = Env::new();
    env.write(
        ".proserunner/config.toml",
        "schema = 1\n[formats]\n\"*.mdx\" = \"markdown\"\n\"legacy/\" = \"org\"\n",
    );
    env.write("docs/a.mdx", DIRTY);
    env.write("docs/b.rs", DIRTY);
    // As Org, `=utilize=` is verbatim.
    env.write("legacy/c.md", "We =utilize= synergy.\n");
    let r = run(env.cmd().args(["docs", "legacy", "-o", "plain"]));
    assert_eq!(r.code, 1, "{}", r.stderr);
    assert!(r.stdout.contains("docs/a.mdx:1:4: [1]"), "{}", r.stdout);
    assert!(!r.stdout.contains("b.rs"), "{}", r.stdout);
    let legacy: Vec<&str> = r.stdout.lines().filter(|l| l.contains("legacy/")).collect();
    assert_eq!(legacy.len(), 1, "{}", r.stdout);
    assert!(legacy[0].contains("\"synergy\""), "{}", r.stdout);

    // Named files go by the same rows; unmapped ones are still an error.
    assert_eq!(run(env.cmd().arg("docs/a.mdx")).code, 1);
    let r = run(env.cmd().arg("docs/b.rs"));
    assert_eq!(r.code, 2);
    assert!(r.stderr.contains("[formats]"), "{}", r.stderr);

    env.write(
        ".proserunner/config.toml",
        "schema = 1\n[formats]\n\"*.mdx\" = \"markdwn\"\n",
    );
    let r = run(env.cmd().arg("docs"));
    assert_eq!(r.code, 2);
    assert_eq!(
        r.stderr,
        "proserunner: .proserunner/config.toml: [formats] \"*.mdx\": no format is named `markdwn` (did you mean `markdown`?)\n"
    );
}

#[test]
fn stdin_filename_finds_the_project_and_applies_its_settings() {
    let env = Env::new();
    // The project is under the working directory, so only the name finds it.
    env.write(
        "site/.proserunner/config.toml",
        "schema = 1\n[[overrides]]\npaths = [\"legal/\"]\nrule.\"default/corporate-speak\".level = \"off\"\n",
    );
    let found = |name: &str, more: &[&str]| {
        let r = run(env
            .cmd()
            .args(["--stdin-filename", name, "-o", "json"])
            .args(more)
            .write_stdin(DIRTY));
        assert!(r.code < 2, "{}", r.stderr);
        let v: Value = serde_json::from_str(&r.stdout).unwrap();
        v["issues"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["rule"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    };
    let speak = "default/corporate-speak".to_owned();
    assert!(found("site/docs/a.md", &[]).contains(&speak));
    assert!(!found("site/legal/a.md", &[]).contains(&speak));

    // Issues get a file, so they can be ignored by number.
    let r = run(env
        .cmd()
        .args(["--stdin-filename", "site/docs/a.md", "--ignore-issues", "1"])
        .write_stdin(DIRTY));
    assert_eq!(r.code, 1, "{}", r.stderr);
    let list = env.read("site/.proserunner/ignores.toml");
    assert!(
        list.contains(
            "file = \"docs/a.md\"\nrule = \"default/needless-variants\"\nmatch = \"utilize\"\n"
        ),
        "{list}"
    );
    let variants = "default/needless-variants".to_owned();
    assert!(!found("site/docs/a.md", &[]).contains(&variants));
    assert!(found("site/docs/b.md", &[]).contains(&variants));
    assert!(found("site/docs/a.md", &["--no-ignores"]).contains(&variants));
}

#[test]
fn config_errors_name_the_file_and_line() {
    let env = Env::new();
    env.write("a.md", CLEAN);
    env.write(
        ".proserunner/config.toml",
        "schema = 1\n\n[check]\nquoted-txt = true\n",
    );
    let r = run(env.cmd().arg("a.md"));
    assert_eq!(r.code, 2);
    assert!(
        r.stderr
            .starts_with("proserunner: .proserunner/config.toml: "),
        "{}",
        r.stderr
    );
    assert!(r.stderr.contains("line 4"), "{}", r.stderr);
    assert!(
        r.stderr.contains("unknown field `quoted-txt`"),
        "{}",
        r.stderr
    );

    env.write(
        ".proserunner/config.toml",
        "schema = 1\n[rule.\"default/hedgin\"]\nlevel = \"off\"\n",
    );
    let r = run(env.cmd().arg("a.md"));
    assert_eq!(r.code, 2);
    assert_eq!(
        r.stderr,
        "proserunner: .proserunner/config.toml: no rule has the id `default/hedgin` (did you mean `default/hedging`?)\n"
    );

    env.write(
        ".proserunner/config.toml",
        "schema = 1\nrules = [\"missing\"]\n",
    );
    let r = run(env.cmd().arg("a.md"));
    assert_eq!(r.code, 2);
    assert!(
        r.stderr
            .contains("the rule directory missing doesn't exist"),
        "{}",
        r.stderr
    );
}

#[test]
fn explicit_config_goes_on_top_and_inherit_false_drops_the_rest() {
    let env = Env::new();
    env.write(
        &format!("{}/config.toml", Env::CONFIG),
        "schema = 1\naccept = [\"utilize\"]\n",
    );
    env.write("dirty.md", DIRTY);
    env.write(
        "strict.toml",
        "schema = 1\n[rule.\"default/jargon\"]\nlevel = \"off\"\n",
    );
    assert_eq!(
        rules_found(&env, &["dirty.md", "--config", "strict.toml"]),
        ["default/corporate-speak", "default/compression"]
    );
    env.write("alone.toml", "schema = 1\ninherit = false\n");
    assert_eq!(
        rules_found(&env, &["dirty.md", "--config", "alone.toml"]).len(),
        4
    );
    let r = run(env.cmd().args(["dirty.md", "--config", "missing.toml"]));
    assert_eq!(r.code, 2);
    assert!(
        r.stderr.contains("missing.toml: no such file"),
        "{}",
        r.stderr
    );
}

#[test]
fn project_rules_get_their_prefix() {
    let env = Env::new();
    let r = run(env.cmd().arg("init"));
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(
        r.stderr,
        "Created .proserunner/config.toml and .proserunner/rules/.\nNext, check the project: proserunner .\n"
    );
    assert!(env.path(".proserunner/rules").is_dir());
    env.write(
        ".proserunner/rules/style.toml",
        "schema = 1\nmessage = \"House style.\"\nwords = [\"e-mail\"]\n[examples]\nbad = [\"An e-mail.\"]\n",
    );
    let r = run(env.cmd().arg("rules"));
    assert!(r.stdout.contains("project/style"), "{}", r.stdout);
    assert_eq!(r.stdout.lines().count(), 23);
    let r = run(env.cmd().args(["rules", "test"]));
    assert_eq!(r.stderr, "23 rules; every example passes.\n");

    let r = run(env.cmd().arg("init"));
    assert_eq!(r.code, 2);
    assert!(r.stderr.contains("already exists"), "{}", r.stderr);
}

// ---------------------------------------------------------------------
// Ignores.

#[test]
fn ignore_add_list_and_remove() {
    let env = Env::new();
    env.write("dirty.md", DIRTY);
    let r = run(env
        .cmd()
        .args(["ignore", "add", "synergy", "due to  the fact that"]));
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(
        r.stderr,
        "Added “synergy” and “due to the fact that” to the global ignore list.\n"
    );
    assert_eq!(
        rules_found(&env, &["dirty.md"]),
        ["default/needless-variants", "default/jargon"]
    );

    let r = run(env.cmd().args(["ignore", "add", "Synergy"]));
    assert_eq!(r.stderr, "Already ignored: “Synergy”.\n");

    let r = run(env.cmd().args(["ignore", "list"]));
    assert_eq!(
        r.stdout,
        "~/.config/proserunner/ignores.toml (global)\n  \"synergy\"\n  \"due to the fact that\"\n"
    );

    let r = run(env.cmd().args(["ignore", "rm", "SYNERGY", "absent"]));
    assert_eq!(r.code, 0);
    assert_eq!(
        r.stderr,
        "proserunner: not in the global ignore list: “absent”\nRemoved “SYNERGY” from the global ignore list.\n"
    );
    assert_eq!(rules_found(&env, &["dirty.md"]).len(), 3);
    assert_eq!(rules_found(&env, &["dirty.md", "--no-ignores"]).len(), 4);
}

#[test]
fn ignore_issues_by_number_and_all() {
    let env = Env::new();
    env.write(".proserunner/config.toml", "schema = 1\n");
    env.write("dirty.md", DIRTY);
    let r = run(env
        .cmd()
        .args(["dirty.md", "--ignore-issues", "2,4", "-o", "plain"]));
    // What's left is reported, renumbered, and still fails the run.
    assert_eq!(r.code, 1, "{}", r.stderr);
    assert_eq!(
        r.stdout,
        "dirty.md:1:4: [1] \"utilize\" -> Prefer: use.\n\
         dirty.md:3:11: [2] \"due to the fact that\" -> Prefer: because, since, why.\n"
    );
    assert_eq!(
        r.stderr,
        "Ignored 2 issues in the project ignore list (.proserunner/ignores.toml), each wherever its match is in its file:\n  \
         [2] \"synergy\" in dirty.md (default/corporate-speak)\n  \
         [4] \"per your request\" in dirty.md (default/jargon)\n"
    );
    let list = env.read(".proserunner/ignores.toml");
    assert!(
        list.contains("[[issue]]\nfile = \"dirty.md\"\nrule = \"default/corporate-speak\"\nmatch = \"synergy\"\n"),
        "{list}"
    );
    assert_eq!(
        rules_found(&env, &["dirty.md"]),
        ["default/needless-variants", "default/compression"]
    );

    // Out-of-range numbers are an error, and nothing is written.
    let r = run(env.cmd().args(["dirty.md", "--ignore-issues", "1,9"]));
    assert_eq!(r.code, 2);
    assert_eq!(
        r.stderr,
        "proserunner: --ignore-issues: there's no issue 9; the last run found 2 (1-2)\n"
    );
    assert_eq!(env.read(".proserunner/ignores.toml"), list);
    let r = run(env.cmd().args(["dirty.md", "--ignore-issues", "x"]));
    assert_eq!(r.code, 2);

    let r = run(env.cmd().args(["dirty.md", "--ignore-all", "--global"]));
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(env.path(&format!("{}/ignores.toml", Env::CONFIG)).exists());
    assert_eq!(run(env.cmd().arg("dirty.md")).code, 0);
    let r = run(env.cmd().args(["-", "--ignore-all"]).write_stdin(DIRTY));
    assert_eq!(r.code, 2);
    assert!(r.stderr.contains("standard input"), "{}", r.stderr);
    let r = run(env.cmd().args(["dirty.md", "--global"]));
    assert_eq!(r.code, 2);
}

/// Numbers are the ones the last run of the same paths showed, so fixing
/// one issue and then ignoring others by number ignores the ones shown,
/// though fixing it moved every number after it.
#[test]
fn ignore_issues_takes_the_numbers_the_last_run_showed() {
    let env = Env::new();
    env.write(".proserunner/config.toml", "schema = 1\n");
    env.write("dirty.md", DIRTY);
    let r = run(env.cmd().args(["dirty.md", "-o", "plain"]));
    assert!(r.stdout.contains("[2] \"synergy\""), "{}", r.stdout);
    assert!(
        r.stdout.contains("[3] \"due to the fact that\""),
        "{}",
        r.stdout
    );

    env.write("dirty.md", &DIRTY.replace("utilize", "use"));
    let r = run(env.cmd().args(["dirty.md", "--ignore-issues", "2,3"]));
    assert_eq!(r.code, 1, "{}", r.stderr);
    assert!(
        r.stderr.contains("[2] \"synergy\" in dirty.md"),
        "{}",
        r.stderr
    );
    assert!(
        r.stderr
            .contains("[3] \"due to the fact that\" in dirty.md"),
        "{}",
        r.stderr
    );
    assert_eq!(rules_found(&env, &["dirty.md"]), ["default/jargon"]);

    // An issue fixed since the last run is skipped, and says so.
    env.write("dirty.md", "We utilize it.\n");
    run(env.cmd().arg("dirty.md"));
    env.write("dirty.md", "We use it.\n");
    let r = run(env.cmd().args(["dirty.md", "--ignore-issues", "1"]));
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(
        r.stderr,
        "[1] \"utilize\" in dirty.md (default/needless-variants) isn't there any more.\n\
         No issues to ignore.\nChecked 1 file and found no issues.\n"
    );

    // After a run of other paths, numbers are this run's.
    env.write("dirty.md", DIRTY);
    env.write("other.md", "We utilize synergy.\n");
    run(env.cmd().arg("other.md"));
    let r = run(env.cmd().args(["dirty.md", "--ignore-issues", "1"]));
    assert!(
        r.stderr.contains("[1] \"utilize\" in dirty.md"),
        "{}",
        r.stderr
    );
}

#[test]
fn ignore_audit_clean_and_clear() {
    let env = Env::new();
    env.write(".proserunner/config.toml", "schema = 1\n");
    env.write(
        ".proserunner/ignores.toml",
        "schema = 1\n# Kept on purpose.\naccept = [\"hopefully\"]\n\n[[issue]]\nfile = \"gone.md\"\nmatch = \"x\"\n\n[[issue]]\nfile = \"here.md\"\nmatch = \"y\"\n",
    );
    env.write("here.md", CLEAN);
    let r = run(env.cmd().args(["ignore", "audit"]));
    assert_eq!(r.code, 1);
    assert_eq!(r.stdout, "gone.md: \"x\"\n");

    let r = run(env.cmd().args(["ignore", "clean"]));
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(
        r.stderr,
        "Removed 1 stale ignore from .proserunner/ignores.toml.\n"
    );
    let text = env.read(".proserunner/ignores.toml");
    assert!(text.contains("# Kept on purpose."), "{text}");
    assert!(!text.contains("gone.md"), "{text}");
    assert_eq!(run(env.cmd().args(["ignore", "audit"])).code, 0);

    // Not in a terminal, clearing needs --force.
    let r = run(env.cmd().args(["ignore", "clear"]));
    assert_eq!(r.code, 2);
    assert!(r.stderr.contains("needs --force"), "{}", r.stderr);
    let r = run(env.cmd().args(["ignore", "clear", "--force"]));
    assert_eq!(
        r.stderr,
        "Cleared 2 ignores from .proserunner/ignores.toml.\n"
    );
    let r = run(env.cmd().args(["ignore", "clear", "--force"]));
    assert_eq!(r.stderr, "The project ignore list is already empty.\n");
}

#[test]
fn relative_global_ignores_are_never_stale() {
    let env = Env::new();
    env.write("docs/a.md", DIRTY);
    let list = format!("{}/ignores.toml", Env::CONFIG);
    env.write(
        &list,
        "schema = 1\n\n[[issue]]\nfile = \"docs/a.md\"\nmatch = \"synergy\"\n\n[[issue]]\nfile = \"~/gone.md\"\nmatch = \"x\"\n",
    );
    // From another directory, `docs/a.md` isn't there, but it's still
    // what the entry means from the root.
    let r = run(env
        .cmd_in("elsewhere")
        .args(["ignore", "audit", "--global"]));
    assert_eq!(r.stdout, "~/gone.md: \"x\"\n");
    let r = run(env
        .cmd_in("elsewhere")
        .args(["ignore", "clean", "--global"]));
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(env.read(&list).contains("docs/a.md"));
    assert!(!env.read(&list).contains("gone.md"));
    assert_eq!(rules_found(&env, &["docs/a.md"]).len(), 3);
}

#[test]
fn ignore_add_refuses_a_list_the_config_leaves_out() {
    let env = Env::new();
    env.write("strict.toml", "schema = 1\ninherit = false\n");
    env.write("dirty.md", DIRTY);
    let r = run(env
        .cmd()
        .args(["-c", "strict.toml", "ignore", "add", "synergy"]));
    assert_eq!(r.code, 2);
    assert!(
        r.stderr.contains("leaves the global ignore list out"),
        "{}",
        r.stderr
    );
    assert!(!env.path(&format!("{}/ignores.toml", Env::CONFIG)).exists());

    // Naming the list adds to it: runs without the config use it.
    let r = run(env
        .cmd()
        .args(["-c", "strict.toml", "ignore", "add", "synergy", "--global"]));
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(rules_found(&env, &["dirty.md"]).len(), 3);
}

#[test]
fn inherit_false_doesnt_read_the_configs_it_leaves_out() {
    let env = Env::new();
    env.write(&format!("{}/config.toml", Env::CONFIG), "schema = 2\n");
    env.write(".proserunner/config.toml", "schema = 1\ninherit = false\n");
    env.write("dirty.md", DIRTY);
    assert_eq!(rules_found(&env, &["dirty.md"]).len(), 4);

    env.write(".proserunner/config.toml", "schema = 1\n");
    let r = run(env.cmd().arg("dirty.md"));
    assert_eq!(r.code, 2);
    assert!(r.stderr.contains("schema 2"), "{}", r.stderr);
}

#[test]
fn config_excludes_ignore_the_directories_a_run_is_in() {
    let env = Env::new();
    env.write(
        &format!("{}/config.toml", Env::CONFIG),
        "schema = 1\n[check]\nexclude = [\"drafts\"]\n",
    );
    env.write("drafts/novel/a.md", DIRTY);
    env.write("drafts/novel/drafts/b.md", DIRTY);
    let r = run(env.cmd_in("drafts/novel").args([".", "-o", "json"]));
    assert_eq!(r.code, 1, "{}", r.stderr);
    let found: Value = serde_json::from_str(&r.stdout).unwrap();
    let files: Vec<&str> = found["issues"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|i| i["file"].as_str())
        .collect();
    assert!(files.iter().all(|f| *f == "a.md"), "{files:?}");
}

#[cfg(unix)]
#[test]
fn project_ignores_need_no_writable_home() {
    use std::os::unix::fs::PermissionsExt as _;

    let env = Env::new();
    env.write(".proserunner/config.toml", "schema = 1\n");
    env.write("dirty.md", DIRTY);
    let home = env.path("home");
    fs::create_dir_all(&home).unwrap();
    fs::set_permissions(&home, fs::Permissions::from_mode(0o555)).unwrap();
    if fs::create_dir(home.join("probe")).is_ok() {
        return; // Permissions don't bind this user (root).
    }
    let r = run(env.cmd().args(["ignore", "add", "synergy"]));
    fs::set_permissions(&home, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(env.read(".proserunner/ignores.toml").contains("synergy"));
}

#[test]
fn project_ignores_skip_global_ones_under_inherit_false() {
    let env = Env::new();
    env.write(
        &format!("{}/ignores.toml", Env::CONFIG),
        "schema = 1\naccept = [\"synergy\"]\n",
    );
    env.write(".proserunner/config.toml", "schema = 1\n");
    env.write("dirty.md", DIRTY);
    assert_eq!(rules_found(&env, &["dirty.md"]).len(), 3);
    env.write(".proserunner/config.toml", "schema = 1\ninherit = false\n");
    assert_eq!(rules_found(&env, &["dirty.md"]).len(), 4);
}

#[test]
fn ignores_never_go_to_a_list_inherit_false_leaves_out() {
    let env = Env::new();
    env.write(".proserunner/config.toml", "schema = 1\n");
    env.write("alone.toml", "schema = 1\ninherit = false\n");
    env.write("dirty.md", DIRTY);
    let r = run(env
        .cmd()
        .args(["--config", "alone.toml", "dirty.md", "--ignore-issues", "1"]));
    assert_eq!(r.code, 2);
    assert!(
        r.stderr
            .contains("--ignore-issues: config sets `inherit = false`, which leaves the project ignore list out of this run"),
        "{}",
        r.stderr
    );
    assert!(!env.path(".proserunner/ignores.toml").exists());

    // A project's inherit = false leaves out the global list, not its own.
    env.write(".proserunner/config.toml", "schema = 1\ninherit = false\n");
    let r = run(env.cmd().args(["dirty.md", "--ignore-all", "--global"]));
    assert_eq!(r.code, 2);
    assert!(
        r.stderr.contains("leaves the global ignore list out"),
        "{}",
        r.stderr
    );
    let r = run(env.cmd().args(["dirty.md", "--ignore-all"]));
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert!(rules_found(&env, &["dirty.md"]).is_empty());
}

#[test]
fn patterns_without_a_slash_never_match_above_the_project() {
    let env = Env::new();
    env.write(
        "docs/site/.proserunner/config.toml",
        "schema = 1\n[check]\nexclude = [\"docs/\"]\n",
    );
    env.write("docs/site/a.md", DIRTY);
    env.write("docs/site/docs/b.md", DIRTY);
    let r = run(env.cmd_in("docs/site").args([".", "-o", "plain"]));
    assert_eq!(r.code, 1, "{}", r.stderr);
    assert!(r.stdout.starts_with("a.md:"), "{}", r.stdout);
    assert!(!r.stdout.contains("b.md"), "{}", r.stdout);
}

// ---------------------------------------------------------------------
// 0.8 bugs that must not come back.

/// B1: project ignore commands used the working directory as the project
/// root, so running from a subdirectory wrote the wrong file with the
/// wrong path.
#[test]
fn b1_project_ignores_use_the_project_root_from_any_subdirectory() {
    let env = Env::new();
    env.write(".proserunner/config.toml", "schema = 1\n");
    env.write("docs/guide/dirty.md", DIRTY);
    let r = run(env
        .cmd_in("docs/guide")
        .args(["dirty.md", "--ignore-issues", "1"]));
    assert_eq!(r.code, 1, "{}", r.stderr);
    assert!(!env.path("docs/guide/.proserunner").exists());
    let list = env.read(".proserunner/ignores.toml");
    assert!(list.contains("file = \"docs/guide/dirty.md\""), "{list}");

    let found_in = |dir: &str, path: &str| {
        let r = run(env.cmd_in(dir).args([path, "-o", "plain"]));
        r.stdout.lines().count()
    };
    assert_eq!(found_in("docs/guide", "dirty.md"), 3);
    assert_eq!(found_in("", "docs/guide/dirty.md"), 3);
    assert_eq!(found_in("docs", "guide/dirty.md"), 3);
}

/// B2: `ignore clean` wrote a vector where the project config required a
/// set, so the next run rejected the file.
#[test]
fn b2_clean_writes_a_file_the_next_run_reads() {
    let env = Env::new();
    env.write(".proserunner/config.toml", "schema = 1\n");
    env.write("dirty.md", DIRTY);
    assert_eq!(run(env.cmd().args(["dirty.md", "--ignore-all"])).code, 0);
    fs::remove_file(env.path("dirty.md")).unwrap();
    env.write("dirty2.md", DIRTY);
    assert_eq!(run(env.cmd().args(["ignore", "clean"])).code, 0);
    let r = run(env.cmd().args(["dirty2.md", "-o", "plain"]));
    assert_eq!(r.code, 1, "{}", r.stderr);
    assert_eq!(r.stdout.lines().count(), 4);
    assert_eq!(
        run(env.cmd().args(["ignore", "list"])).stderr,
        "No ignores.\n"
    );
}

/// B3: a file that didn't parse was replaced whole by the next write.
#[test]
fn b3_a_broken_file_is_never_overwritten() {
    let env = Env::new();
    let broken = "schema = 1\naccept = [\"a\"\n";
    let path = format!("{}/ignores.toml", Env::CONFIG);
    env.write(&path, broken);
    env.write("dirty.md", DIRTY);
    for args in [
        vec!["ignore", "add", "b"],
        vec!["ignore", "clear", "--force"],
        vec!["dirty.md", "--ignore-all"],
    ] {
        let r = run(env.cmd().args(&args));
        assert_eq!(r.code, 2, "{args:?}");
        assert!(r.stderr.contains("ignores.toml"), "{}", r.stderr);
        assert_eq!(env.read(&path), broken, "{args:?}");
    }
}

/// B4: the default checks could download on every run. Now the rules are
/// built in, and a check writes nothing but the issues it numbered, in the
/// cache directory, for `--ignore-issues`.
#[test]
fn b4_a_check_with_no_config_writes_only_its_cache() {
    let env = Env::new();
    env.write("dirty.md", DIRTY);
    let before = files(env.root());
    assert_eq!(run(env.cmd().arg("dirty.md")).code, 1);
    assert_eq!(run(env.cmd().arg("rules")).code, 0);
    let mut after = files(env.root());
    after.retain(|f| f != "home/.cache/proserunner/last-run.json");
    assert_eq!(after, before);
}

/// B5: `:files` behaved differently in the global and project configs.
/// Turning a rule off is the same setting in every layer.
#[test]
fn b5_turning_a_rule_off_works_the_same_in_every_layer() {
    let off = "schema = 1\n[rule.\"default/jargon\"]\nlevel = \"off\"\n";
    let user = format!("{}/config.toml", Env::CONFIG);
    let mut results = Vec::new();
    for (layer, args) in [
        (user.as_str(), vec!["dirty.md"]),
        (".proserunner/config.toml", vec!["dirty.md"]),
        ("x.toml", vec!["dirty.md", "--config", "x.toml"]),
    ] {
        let env = Env::new();
        env.write(layer, off);
        env.write("dirty.md", DIRTY);
        results.push(rules_found(&env, &args));
    }
    assert_eq!(results[0], results[1]);
    assert_eq!(results[1], results[2]);
    assert!(!results[0].iter().any(|r| r == "default/jargon"));
}

/// B6: whether a path was inside the project was a string-prefix test, so
/// `/x/proj-old` counted as inside `/x/proj`.
#[test]
fn b6_a_sibling_with_the_same_prefix_isnt_inside_the_project() {
    let env = Env::new();
    env.write("proj/.proserunner/config.toml", "schema = 1\n");
    env.write("proj-old/dirty.md", DIRTY);
    let r = run(env
        .cmd_in("proj")
        .args(["../proj-old/dirty.md", "--ignore-issues", "1"]));
    assert_eq!(r.code, 1, "{}", r.stderr);
    let list = env.read("proj/.proserunner/ignores.toml");
    let expected = format!("file = \"{}\"", env.path("proj-old/dirty.md").display());
    assert!(list.contains(&expected), "{list}");
    let r = run(env
        .cmd_in("proj")
        .args(["../proj-old/dirty.md", "-o", "plain"]));
    assert_eq!(r.stdout.lines().count(), 3);
}

/// B7: `checks restore` never updated `:files`, so new default checks
/// stayed off. Rules are on unless config turns them off.
#[test]
fn b7_rules_not_named_in_config_stay_on() {
    let env = Env::new();
    env.write(
        &format!("{}/config.toml", Env::CONFIG),
        "schema = 1\n[rule.\"default/jargon\"]\nlevel = \"off\"\n",
    );
    let r = run(env.cmd().arg("rules"));
    let levels: Vec<&str> = r
        .stdout
        .lines()
        .map(|l| l.split_whitespace().nth(1).unwrap())
        .collect();
    assert_eq!(levels.len(), 22);
    assert_eq!(levels.iter().filter(|l| **l == "off").count(), 1);
    let jargon = r
        .stdout
        .lines()
        .find(|l| l.starts_with("default/jargon "))
        .unwrap();
    assert_eq!(jargon.split_whitespace().nth(1), Some("off"));
}

/// The 0.8 cache lost the issues of duplicated lines and kept a moved
/// line's old file. There's no cache now; the cases stay covered.
#[test]
fn duplicate_lines_and_identical_files_all_report() {
    let env = Env::new();
    env.write("a.md", "We utilize it.\n\nWe utilize it.\n");
    env.write("b.md", "We utilize it.\n\nWe utilize it.\n");
    let r = run(env.cmd().args(["a.md", "b.md", "-o", "plain"]));
    let places: Vec<&str> = r
        .stdout
        .lines()
        .map(|l| l.split(" [").next().unwrap())
        .collect();
    assert_eq!(places, ["a.md:1:4:", "a.md:3:4:", "b.md:1:4:", "b.md:3:4:"]);
}

/// 0.8's `checks` crashed on a check without an explanation.
#[test]
fn rules_without_explanations_list() {
    let env = Env::new();
    env.write(
        ".proserunner/rules/bare.toml",
        "schema = 1\nmessage = \"m\"\nwords = [\"x\"]\n",
    );
    env.write(
        ".proserunner/config.toml",
        "schema = 1\nrules = [{ path = \".proserunner/rules\", prefix = \"project\" }]\n",
    );
    let r = run(env.cmd().arg("rules"));
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(r.stdout, "project/bare  warning  Bare\n");
}

// ---------------------------------------------------------------------
// Migration from 0.8.

#[test]
fn migrates_08_user_settings_once() {
    let env = Env::new();
    let dir = Env::CONFIG;
    env.write(
        &format!("{dir}/config.edn"),
        r#";; Proserunner Global Configuration
{:checks
 [{:name "default", :directory "default",
   :files ["annotations" "archaisms" "cliches" "compression" "corporate-speak" "hedging"
           "meta-discourse" "needless-variants" "nominalization" "non-words"
           "not-the-negative" "overused-adverbs" "oxymorons" "phrasal-adjectives"
           "pompous-diction" "redundancies" "regex" "repetition" "sexism"
           "skunked-terms" "weasel-words"]}
  {:name "Custom checks: style" :directory "custom/style" :files ["house"]}]}"#,
    );
    env.write(&format!("{dir}/default/jargon.edn"), "{:name \"jargon\"}");
    env.write(
        &format!("{dir}/custom/style/house.edn"),
        r#"{:name "house" :kind "recommender" :message "x"
            :recommendations [{:avoid "e-mail" :prefer "email"}]}"#,
    );
    env.write(
        &format!("{dir}/custom/style/old.edn"),
        r#"{:name "old" :kind "existence" :message "Old." :specimens ["olde"]}"#,
    );
    env.write(
        &format!("{dir}/ignore.edn"),
        r#"{:ignore #{"hopefully"}
            :ignore-issues [{:file "docs/a.md" :line-num 3 :specimen "utilize" :check "needless-variants"}]}"#,
    );
    env.write(
        "doc.md",
        "Send an e-mail, hopefully, due to the fact that olde.\n",
    );

    let r = run(env.cmd().args(["doc.md", "-o", "plain"]));
    assert_eq!(r.code, 1, "{}", r.stderr);
    assert!(
        r.stderr.starts_with(
            "Migrated proserunner 0.8 settings: wrote ~/.config/proserunner/config.toml, \
             ~/.config/proserunner/ignores.toml and 2 rule files. The 0.8 files are now *.edn.bak.\n"
        ),
        "{}",
        r.stderr
    );
    assert!(
        r.stderr.contains("default holds the checks 0.8 downloaded"),
        "{}",
        r.stderr
    );
    // jargon was left out of :files, so it's off; old.edn wasn't listed.
    assert_eq!(
        r.stdout,
        "doc.md:1:9: [1] \"e-mail\" -> Prefer: email.\n\
         doc.md:1:28: [2] \"due to the fact that\" -> Prefer: because, since, why.\n"
    );

    let config = env.read(&format!("{dir}/config.toml"));
    assert!(
        config.contains("rules = [\"default\", \"custom/style\"]"),
        "{config}"
    );
    assert!(
        config.contains("[rule.\"default/jargon\"]\nlevel = \"off\""),
        "{config}"
    );
    assert!(
        config.contains("[rule.\"style/old\"]\nlevel = \"off\""),
        "{config}"
    );
    let ignores = env.read(&format!("{dir}/ignores.toml"));
    assert!(
        ignores.contains("file = \"docs/a.md\"\nrule = \"default/needless-variants\"\nmatch = \"utilize\"\nline = 3"),
        "{ignores}"
    );
    for kept in [
        "config.edn.bak",
        "ignore.edn.bak",
        "custom/style/house.edn.bak",
    ] {
        assert!(env.path(&format!("{dir}/{kept}")).exists(), "{kept}");
    }

    // Once is enough.
    let r = run(env.cmd().args(["doc.md", "-o", "plain"]));
    assert!(!r.stderr.contains("Migrated"), "{}", r.stderr);
}

#[test]
fn runs_that_start_together_migrate_once() {
    let env = Env::new();
    let dir = Env::CONFIG;
    env.write(
        &format!("{dir}/config.edn"),
        r#"{:checks [{:name "default" :directory "default"}]}"#,
    );
    env.write(&format!("{dir}/ignore.edn"), r#"#{"synergy"}"#);
    env.write("dirty.md", DIRTY);

    let children: Vec<_> = (0..4)
        .map(|_| {
            env.process()
                .arg("dirty.md")
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap()
        })
        .collect();
    let mut migrated = 0;
    for child in children {
        let out = child.wait_with_output().unwrap();
        let stderr = String::from_utf8(out.stderr).unwrap();
        assert_eq!(out.status.code(), Some(1), "{stderr}");
        migrated += usize::from(stderr.contains("Migrated"));
    }
    assert_eq!(migrated, 1);
}

#[test]
fn migrates_an_08_project() {
    let env = Env::new();
    env.write(
        ".proserunner/config.edn",
        r#";; Proserunner Project Configuration
{:checks ["default" "checks"]
 :ignore #{"synergy"}
 :ignore-issues #{{:file "dirty.md" :line-num 1 :specimen "utilize"}}
 :ignore-mode :replace
 :config-mode :merged}"#,
    );
    env.write(
        ".proserunner/checks/rained.edn",
        r#"{:name "rained" :kind "existence" :message "Weather." :specimens ["it rained"]}"#,
    );
    env.write(
        &format!("{}/ignores.toml", Env::CONFIG),
        "schema = 1\naccept = [\"due to the fact that\"]\n",
    );
    env.write("dirty.md", DIRTY);

    let found = rules_found(&env, &["dirty.md"]);
    // inherit = false: the global ignore of "due to the fact that" is out.
    assert_eq!(
        found,
        ["default/compression", "checks/rained", "default/jargon"]
    );
    let config = env.read(".proserunner/config.toml");
    assert!(config.contains("inherit = false"), "{config}");
    assert!(
        config.contains("rules = [\"default\", \".proserunner/checks\"]"),
        "{config}"
    );
    assert!(env.path(".proserunner/config.edn.bak").exists());
    assert!(env.path(".proserunner/checks/rained.toml").exists());
}

#[test]
fn migrates_the_bare_set_ignore_file() {
    let env = Env::new();
    env.write(
        &format!("{}/ignore.edn", Env::CONFIG),
        ";; Put specimens for Proserunner to ignore between the braces.\n#{\"synergy\"\n  ;; \"example\"\n  }\n",
    );
    env.write("dirty.md", DIRTY);
    assert_eq!(rules_found(&env, &["dirty.md"]).len(), 3);
    let list = env.read(&format!("{}/ignores.toml", Env::CONFIG));
    assert!(list.contains("accept = [\"synergy\"]"), "{list}");
}

#[test]
fn a_malformed_08_file_stops_migration_with_no_changes() {
    let env = Env::new();
    let dir = Env::CONFIG;
    env.write(
        &format!("{dir}/config.edn"),
        "{:checks [{:directory \"custom\"}]}",
    );
    env.write(
        &format!("{dir}/custom/good.edn"),
        r#"{:name "good" :kind "existence" :message "m" :specimens ["x"]}"#,
    );
    env.write(&format!("{dir}/ignore.edn"), "{:ignore #{\"a\" ");
    env.write("a.md", CLEAN);
    let before = files(env.root());
    let r = run(env.cmd().arg("a.md"));
    assert_eq!(r.code, 2);
    assert!(r.stderr.contains("ignore.edn: line 1"), "{}", r.stderr);
    assert!(r.stderr.contains("changes nothing"), "{}", r.stderr);
    assert_eq!(files(env.root()), before);

    env.write(&format!("{dir}/ignore.edn"), "#{}");
    env.write(
        &format!("{dir}/custom/bad.edn"),
        r#"{:name "bad" :kind "grammar"}"#,
    );
    let r = run(env.cmd().arg("a.md"));
    assert_eq!(r.code, 2);
    assert!(
        r.stderr.contains("bad.edn: unknown :kind \"grammar\""),
        "{}",
        r.stderr
    );
    assert!(!env.path(&format!("{dir}/config.toml")).exists());
    assert!(!env.path(&format!("{dir}/custom/good.toml")).exists());
}

#[test]
fn a_rule_id_a_typo_away_from_a_loaded_one_is_an_error() {
    let env = Env::new();
    env.write(
        "doc.md",
        "Perhaps.\n<!-- proserunner-disable defualt/jargon -->\n",
    );
    env.write(
        ".proserunner/config.toml",
        "schema = 1\n[rule.\"defualt/hedging\"]\nlevel = \"off\"\n",
    );
    let r = run(env.cmd().arg("doc.md"));
    assert_eq!(r.code, 2);
    assert!(
        r.stderr.contains("did you mean `default/hedging`?"),
        "{}",
        r.stderr
    );

    // The same goes for directives.
    env.write(".proserunner/config.toml", "schema = 1\n");
    let r = run(env.cmd().arg("doc.md"));
    assert_eq!(r.code, 2);
    assert!(r.stderr.contains("doc.md:2:"), "{}", r.stderr);
    assert!(
        r.stderr.contains("did you mean `default/jargon`?"),
        "{}",
        r.stderr
    );
}
