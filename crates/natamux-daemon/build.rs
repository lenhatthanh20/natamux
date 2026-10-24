use std::fs;
use std::io;
use std::path::{Path, PathBuf};

fn files_in(root: &Path, dir: &Path, found: &mut Vec<(String, PathBuf)>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            files_in(root, &path, found)?;
        } else if path.is_file() {
            let name = path
                .strip_prefix(root)
                .map_err(io::Error::other)?
                .to_string_lossy()
                .replace('\\', "/");
            found.push((name, path));
        }
    }
    Ok(())
}

fn main() -> io::Result<()> {
    println!("cargo:rerun-if-env-changed=NATAMUX_EMBED_WEB_DIR");
    println!("cargo:rerun-if-env-changed=NATAMUX_REQUIRE_WEB");
    let root = std::env::var_os("NATAMUX_EMBED_WEB_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("../../packages/web/dist"));
    println!("cargo:rerun-if-changed={}", root.display());
    let mut files = Vec::new();
    if root.is_dir() {
        files_in(&root, &root, &mut files)?;
    }
    if std::env::var_os("NATAMUX_REQUIRE_WEB").is_some()
        && !files.iter().any(|(name, _)| name == "index.html")
    {
        return Err(io::Error::other("web build has no index.html"));
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));
    let mut text = String::from("const EMBEDDED_WEB: &[(&str, &[u8])] = &[\n");
    for (name, path) in files {
        let absolute = fs::canonicalize(&path)?;
        text.push_str(&format!(
            "    ({name:?}, include_bytes!({:?})),\n",
            absolute.to_string_lossy()
        ));
    }
    text.push_str("];\n");
    let out = PathBuf::from(std::env::var_os("OUT_DIR").ok_or_else(|| io::Error::other("OUT_DIR is missing"))?);
    fs::write(out.join("web_assets.rs"), text)
}
