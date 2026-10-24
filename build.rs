//! Embeds every `defaults/*.toml` rule file, so adding a built-in rule
//! needs no code change.

use std::env;
use std::error::Error;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

fn main() -> Result<(), Box<dyn Error>> {
    let manifest = env::var("CARGO_MANIFEST_DIR")?;
    let dir = Path::new(&manifest).join("defaults");
    println!("cargo:rerun-if-changed={}", dir.display());

    let mut stems: Vec<String> = fs::read_dir(&dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "toml"))
        .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .collect();
    stems.sort();

    let mut out = String::from("static BUILTIN: &[BuiltinFile] = &[\n");
    for stem in &stems {
        let path = dir.join(format!("{stem}.toml"));
        println!("cargo:rerun-if-changed={}", path.display());
        writeln!(
            out,
            "    BuiltinFile {{ stem: {stem:?}, text: include_str!({:?}) }},",
            path.display().to_string()
        )?;
    }
    out.push_str("];\n");

    let dest = Path::new(&env::var("OUT_DIR")?).join("builtin.rs");
    fs::write(dest, out)?;
    Ok(())
}
