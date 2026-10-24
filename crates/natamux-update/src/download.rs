//! Stream an asset to a temporary file and publish it only after its signed hash matches.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

struct PartGuard(PathBuf);

impl Drop for PartGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

pub fn save_verified(
    mut source: impl Read,
    target: &Path,
    expected: &str,
    max_bytes: u64,
) -> io::Result<u64> {
    if expected.len() != 64 || !expected.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(io::Error::other("The signed SHA-256 is invalid"));
    }
    let parent = target
        .parent()
        .ok_or_else(|| io::Error::other("No update folder"))?;
    fs::create_dir_all(parent)?;
    let mut part = target.as_os_str().to_owned();
    part.push(".part");
    let part = PartGuard(PathBuf::from(part));
    match fs::remove_file(&part.0) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let mut file = private_new_file(&part.0)?;
    let mut hash = Sha256::new();
    let mut total = 0_u64;
    let mut block = [0_u8; 64 * 1024];
    loop {
        let size = source.read(&mut block)?;
        if size == 0 {
            break;
        }
        total = total.saturating_add(size as u64);
        if total > max_bytes {
            return Err(io::Error::other(
                "The update is larger than the download limit",
            ));
        }
        hash.update(&block[..size]);
        file.write_all(&block[..size])?;
    }
    let actual = hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if !actual.eq_ignore_ascii_case(expected) {
        return Err(io::Error::other(
            "The update does not match signed SHA256SUMS.txt",
        ));
    }
    file.sync_all()?;
    drop(file);
    fs::rename(&part.0, target)?;
    Ok(total)
}

fn private_new_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)
}
