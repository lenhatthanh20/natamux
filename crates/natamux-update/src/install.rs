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
    /// Windows: `Uninstall.exe` sits beside the exe, so the NSIS installer put it there.
    pub uninstaller: bool,
    /// Linux: the exe folder looks like an unpacked `.tar.gz` (`tarball::is_tarball_layout`).
    pub tarball: bool,
}

pub fn mac_bundle(executable: &str) -> Option<&str> {
    let at = executable.find(".app/Contents/MacOS/")?;
    executable.get(..at + ".app".len())
}

/// The AppImage file the running program came from. `APPIMAGE` counts only when the exe is under
/// a non-empty `APPDIR`: a variable inherited from another AppImage names a file that is not ours.
pub fn appimage_for(exe: &Path, appimage: Option<&str>, appdir: Option<&str>) -> Option<String> {
    let appdir = appdir.filter(|dir| !dir.is_empty())?;
    let appimage = appimage.filter(|path| !path.is_empty())?;
    exe.starts_with(appdir).then(|| appimage.to_string())
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
            None if input.tarball && input.writable => InstallKind::Tarball,
            None => InstallKind::Manual,
        },
        "windows" if input.uninstaller && input.writable => InstallKind::Windows,
        _ => InstallKind::Manual,
    }
}

pub fn replace_appimage(verified: &Path, current: &Path) -> io::Result<()> {
    let staged = sibling(current, ".natamux-new")?;
    let backup = sibling(current, ".natamux-old")?;
    if backup.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "An old update backup exists",
        ));
    }
    let result = (|| {
        let mut input = fs::File::open(verified)?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staged)?;
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

pub const HASH_MISMATCH_EXIT: i32 = 3;

/// An old-update backup beside `current` (macOS bundle or AppImage). It may be the only copy of
/// the old app, so the caller shows it and never removes it.
pub fn stale_backup(current: &Path) -> Option<PathBuf> {
    let backup = if current.extension().is_some_and(|ext| ext == "app") {
        let mut name = current.as_os_str().to_os_string();
        name.push(".old");
        PathBuf::from(name)
    } else {
        sibling(current, ".natamux-old").ok()?
    };
    backup.exists().then_some(backup)
}

/// Run as root: `sh -c <this> sh <file> <sha256> <installer>`. The copy in a root-owned folder
/// is the one that gets checked and installed, so the user cannot swap the file in between.
/// A hash mismatch exits with `HASH_MISMATCH_EXIT`, before the installer runs.
pub fn deb_install_script() -> &'static str {
    "set -eu\n\
     dir=$(mktemp -d)\n\
     trap 'rm -rf \"$dir\"' EXIT\n\
     cp \"$1\" \"$dir/pkg\"\n\
     if command -v sha256sum >/dev/null 2>&1; then sum=sha256sum; else sum='shasum -a 256'; fi\n\
     printf '%s  %s\\n' \"$2\" \"$dir/pkg\" | $sum -c - >/dev/null || exit 3\n\
     \"$3\" -i \"$dir/pkg\"\n"
}

fn sibling(path: &Path, suffix: &str) -> io::Result<PathBuf> {
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::other("No app file name"))?;
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
        shell_quote(current),
        shell_quote(staged),
        shell_quote(log)
    )
}

/// Run hidden: `powershell.exe -File <this> <app pid>`. It waits for the app to stop, checks the
/// installer against its signed hash again, then runs the NSIS installer silently into the folder
/// the app runs from. NSIS wants `/D=` last and unquoted; one argument string keeps it so.
/// Every outcome after the wait reopens the app when `relaunch` is set, also a failed check.
/// The BOM makes Windows PowerShell 5.1 read the file as UTF-8, not the ANSI code page, so
/// non-ASCII paths stay intact.
pub fn windows_install_script(
    installer: &str,
    sha256: &str,
    install_dir: &str,
    app_exe: &str,
    log: &str,
    relaunch: bool,
) -> String {
    let reopen = if relaunch {
        "Start-Process -FilePath $app\r\n"
    } else {
        ""
    };
    format!(
        "\u{FEFF}param([int]$AppPid)\r\n\
         $ErrorActionPreference = 'Stop'\r\n\
         $installer = {}\r\n\
         $sha = {}\r\n\
         $dir = {}\r\n\
         $app = {}\r\n\
         $log = {}\r\n\
         function Note([string]$text) {{\r\n\
           try {{ Add-Content -LiteralPath $log -Value $text -Encoding UTF8 }} catch {{ }}\r\n\
         }}\r\n\
         $proc = Get-Process -Id $AppPid -ErrorAction SilentlyContinue\r\n\
         if ($proc -and -not $proc.WaitForExit(120000)) {{ Note 'app did not stop'; exit 1 }}\r\n\
         $code = 1\r\n\
         try {{\r\n\
           if ((Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash -ne $sha) {{\r\n\
             Note 'the installer does not match its hash'\r\n\
             $code = {HASH_MISMATCH_EXIT}\r\n\
           }} else {{\r\n\
             $switches = '/S /D=' + $dir\r\n\
             $run = Start-Process -FilePath $installer -ArgumentList $switches -Wait -PassThru\r\n\
             $code = $run.ExitCode\r\n\
             $note = \"update failed: exit $code\"\r\n\
             if ($code -eq 0) {{ $note = 'installed update' }}\r\n\
             Note $note\r\n\
           }}\r\n\
         }} catch {{\r\n\
           $code = 1\r\n\
           Note \"update failed: $($_.Exception.Message)\"\r\n\
         }}\r\n\
         {reopen}\
         exit $code\r\n",
        ps_quote(installer),
        ps_quote(sha256),
        ps_quote(install_dir),
        ps_quote(app_exe),
        ps_quote(log)
    )
}

/// PowerShell also ends a single-quoted string at the curly quotes, so those are doubled too.
fn ps_quote(text: &str) -> String {
    let mut quoted = String::from("'");
    for ch in text.chars() {
        if matches!(ch, '\'' | '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}') {
            quoted.push(ch);
        }
        quoted.push(ch);
    }
    quoted.push('\'');
    quoted
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
    let copied = Command::new("ditto")
        .arg(mount.join("Natamux.app"))
        .arg(&staged)
        .status();
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
        return Err(io::Error::other(
            "The update app failed the code signature check",
        ));
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
