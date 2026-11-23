//! Windows: adds version info to the exe. Code signing checks the product name and version of
//! every signed file (site: Code signing policy).

// embed-resource is a build dependency on Windows hosts only (Cargo.toml).
#[cfg(not(windows))]
fn main() {
    println!("cargo::rerun-if-changed=build.rs");
}

#[cfg(windows)]
fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    if let Err(error) = embed_version() {
        eprintln!("natamux-daemon build.rs: {error}");
        std::process::exit(1);
    }
}

#[cfg(windows)]
fn embed_version() -> Result<(), String> {
    let out = std::env::var("OUT_DIR").map_err(|e| e.to_string())?;
    let version = std::env::var("CARGO_PKG_VERSION").map_err(|e| e.to_string())?;
    let rc = std::path::PathBuf::from(out).join("natamux-daemon.rc");
    std::fs::write(&rc, resource_script(&version))
        .map_err(|e| format!("cannot write {}: {e}", rc.display()))?;
    embed_resource::compile(&rc, embed_resource::NONE)
        .manifest_required()
        .map_err(|e| e.to_string())
}

#[cfg(windows)]
fn resource_script(version: &str) -> String {
    let mut parts: Vec<u16> = version
        .split(['.', '-', '+'])
        .take(3)
        .map(|part| part.parse().unwrap_or(0))
        .collect();
    parts.resize(4, 0);
    let numbers = parts
        .iter()
        .map(u16::to_string)
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r#"1 VERSIONINFO
FILEVERSION {numbers}
PRODUCTVERSION {numbers}
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "CompanyName", "lenhatthanh20"
      VALUE "FileDescription", "Natamux daemon"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "natamux-daemon"
      VALUE "OriginalFilename", "natamux-daemon.exe"
      VALUE "ProductName", "Natamux"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#
    )
}
