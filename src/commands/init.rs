//! `init`: a project's config and rules directory.

use std::fs;

use proserunner::config;
use proserunner::ctx::{Ctx, PROJECT_DIR};
use proserunner::fsutil;
use proserunner::{Error, Result};

use super::{EXIT_OK, console};

const PROJECT_CONFIG: &str = r#"# proserunner project config. Paths are relative to the project root.
# 'proserunner config' shows the merged config and where each value
# came from; 'proserunner guide config' explains every setting.
schema = 1
rules = ["default", { path = ".proserunner/rules", prefix = "project" }]

# Build on a shared config, such as a team's house style:
# extends = ["../house-style/config.toml"]

# Turn a rule off, or change its level ('proserunner rules set' writes
# these for you):
# [rule."default/hedging"]
# level = "off"
"#;

pub(crate) fn init(ctx: &Ctx) -> Result<u8> {
    if ctx.home.as_deref() == Some(ctx.cwd.as_path()) {
        return Err(Error::Usage(
            "the home directory can't be a project; run 'proserunner init' in a project's directory"
                .into(),
        ));
    }
    let dir = ctx.cwd.join(PROJECT_DIR);
    let path = dir.join(config::FILE);
    if path.exists() {
        return Err(Error::Usage(format!(
            "{} already exists",
            ctx.display(&path)
        )));
    }
    if dir.join("config.edn").exists() {
        return Err(Error::Usage(format!(
            "{} holds proserunner 0.8 settings; run any other command to convert them",
            ctx.display(&dir)
        )));
    }
    let rules_dir = dir.join("rules");
    fs::create_dir_all(&rules_dir).map_err(|e| Error::io(&rules_dir, e))?;
    fsutil::write_atomic(&path, PROJECT_CONFIG)?;
    console::status(
        ctx,
        &format!(
            "Created {} and {}/.",
            ctx.display(&path),
            ctx.display(&rules_dir)
        ),
    );
    console::status(ctx, "Next, check the project: proserunner .");
    console::status(
        ctx,
        "Rules of your own go in .proserunner/rules/; 'proserunner guide rules' shows how to write them.",
    );
    Ok(EXIT_OK)
}
