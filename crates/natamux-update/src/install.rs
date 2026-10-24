//! Choose the supported install path from the running package.

use crate::check::InstallKind;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
#[cfg(target_os = "macos")]
use std::process::Command;

pub struct InstallInput<'a> {
    pub platform: &'a str,
    pub executable: &'a str,
    pub appimage: Option<&'a str>,
    /// Whether the parent folder and bundle or AppImage can be replaced.
    pub writable: bool,
}

pub fn mac_bundle(executable: &str) -> Option<&str> {
    let at = executable.find(".app/Contents/MacOS/")?;
    executable.get(..at + ".app".len())
}

pub fn install_kind(input: InstallInput<'_>) -> InstallKind {
    match input.platform {
        "macos" => match mac_bundle(input.executable) {
            Some(bundle)
                if input.writable
                    && !bundle.starts_with("/Volumes/")
                    && !bundle.contains("/AppTranslocation/") =>
            {
                InstallKind::Mac
            }
            _ => InstallKind::Manual,
        },
        "linux" => match input.appimage.filter(|path| !path.is_empty()) {
            Some(_) if input.writable => InstallKind::AppImage,
            Some(_) => InstallKind::Manual,
            None if input.executable.starts_with("/opt/Natamux/") => InstallKind::Deb,
            None => InstallKind::Manual,
        },
        _ => InstallKind::Manual,
    }
}

pub fn replace_appimage(verified: &Path, current: &Path) -> io::Result<()> {
    let staged = sibling(current, ".natamux-new")?;
    let backup = sibling(current, ".natamux-old")?;
    if backup.exists() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "An old update backup exists"));
    }
    let result = (|| {
        let mut input = fs::File::open(verified)?;
        let mut output = OpenOptions::new().write(true).create_new(true).open(&staged)?;
        io::copy(&mut input, &mut output)?;
        output.flush()?;
        output.sync_all()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&staged, fs::Permissions::from_mode(0o755))?;
        }
        fs::rename(current, &backup)?;
        if let Err(error) = fs::rename(&staged, current) {
            let _ = fs::rename(&backup, current);
            return Err(error);
        }
        let _ = fs::remove_file(verified);
        let _ = fs::remove_file(backup);
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&staged);
    }
    result
}

fn sibling(path: &Path, suffix: &str) -> io::Result<PathBuf> {
    let name = path.file_name().ok_or_else(|| io::Error::other("No app file name"))?;
    let mut name = name.to_os_string();
    name.push(suffix);
    Ok(path.with_file_name(name))
}

pub fn mac_install_script(current: &str, staged: &str, log: &str, relaunch: bool) -> String {
    let reopen = if relaunch { "open \"$current\"\n" } else { "" };
    format!(
        "#!/bin/sh\nset -eu\ncurrent={}\nstaged={}\nlog={}\nbackup=\"$current.old\"\n\
         attempts=0\n\
         while kill -0 \"$1\" 2>/dev/null; do\n\
           attempts=$((attempts + 1))\n\
           if [ \"$attempts\" -ge 120 ]; then echo 'app did not stop' >> \"$log\"; exit 1; fi\n\
           sleep 1\n\
         done\n\
         if [ -e \"$backup\" ]; then echo 'update backup exists' >> \"$log\"; exit 1; fi\n\
         mv \"$current\" \"$backup\"\n\
         if mv \"$staged\" \"$current\"; then\n\
           echo 'installed update' >> \"$log\"\n\
           rm -rf \"$backup\" || true\n\
           {reopen}\
         else\n\
           rm -rf \"$current\"\n\
           mv \"$backup\" \"$current\"\n\
           echo 'update failed; restored old app' >> \"$log\"\n\
           {reopen}\
           exit 1\n\
         fi\n",
        shell_quote(current), shell_quote(staged), shell_quote(log)
    )
}

fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

#[cfg(target_os = "macos")]
pub fn prepare_mac(dmg: &Path, folder: &Path, version: &str) -> io::Result<PathBuf> {
    let mount = folder.join("mnt");
    let staged = folder.join("Natamux.app");
    fs::create_dir_all(&mount)?;
    let attached = Command::new("hdiutil")
        .args(["attach", "-nobrowse", "-readonly", "-mountpoint"])
        .arg(&mount)
        .arg(dmg)
        .status()?;
    if !attached.success() {
        return Err(io::Error::other("The update disk image did not mount"));
    }
    let copied = Command::new("ditto").arg(mount.join("Natamux.app")).arg(&staged).status();
    let detached = Command::new("hdiutil").arg("detach").arg(&mount).status();
    if !copied?.success() || !detached?.success() {
        return Err(io::Error::other("The update app could not be copied"));
    }
    if !Command::new("codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(&staged)
        .status()?
        .success()
    {
        return Err(io::Error::other("The update app failed the code signature check"));
    }
    let output = Command::new("plutil")
        .args(["-extract", "CFBundleShortVersionString", "raw", "-o", "-"])
        .arg(staged.join("Contents/Info.plist"))
        .output()?;
    if !output.status.success() || String::from_utf8_lossy(&output.stdout).trim() != version {
        return Err(io::Error::other("The update app has the wrong version"));
    }
    if !staged.join("Contents/MacOS/natamux-daemon").is_file() {
        return Err(io::Error::other("The update app has no Rust daemon"));
    }
    Ok(staged)
}
