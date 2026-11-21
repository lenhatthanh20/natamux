//! Windows: puts `resources/icon.ico` into the exe as icon 1 and adds version info. gpui loads
//! icon 1 for the window and the taskbar; Explorer and shortcuts show it for the file.

// embed-resource is a build dependency on Windows hosts only (Cargo.toml).
#[cfg(not(windows))]
fn main() {
    println!("cargo::rerun-if-changed=build.rs");
}

#[cfg(windows)]
fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=resources/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    if let Err(error) = windows::embed_icon() {
        eprintln!("natamux-app build.rs: {error}");
        std::process::exit(1);
    }
}

#[cfg(windows)]
mod windows {
    use std::path::PathBuf;

    pub fn embed_icon() -> Result<(), String> {
        let dir = std::env::var("CARGO_MANIFEST_DIR").map_err(|e| e.to_string())?;
        let out = std::env::var("OUT_DIR").map_err(|e| e.to_string())?;
        let version = std::env::var("CARGO_PKG_VERSION").map_err(|e| e.to_string())?;
        let icon = PathBuf::from(dir).join("resources").join("icon.ico");
        let rc = PathBuf::from(out).join("natamux-app.rc");
        std::fs::write(&rc, resource_script(&icon.display().to_string(), &version))
            .map_err(|e| format!("cannot write {}: {e}", rc.display()))?;
        // Required, not optional: with no resource compiler the exe would ship without its icon.
        embed_resource::compile(&rc, embed_resource::NONE)
            .manifest_required()
            .map_err(|e| e.to_string())
    }

    fn resource_script(icon: &str, version: &str) -> String {
        let icon = icon.replace('\\', "\\\\");
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
            r#"1 ICON "{icon}"

1 VERSIONINFO
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
      VALUE "FileDescription", "Natamux"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "natamux-app"
      VALUE "OriginalFilename", "natamux-app.exe"
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
}
