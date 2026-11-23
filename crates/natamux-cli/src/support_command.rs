//! `natamux support-bundle [--out <file>]`: saves the daemon's support bundle (`GET
//! /support-bundle`) to a file only the user can read, to attach to a bug report. The bundle holds
//! no task text, events or environment; read it before sending it.

use std::io::Write;
use std::path::{Path, PathBuf};

use natamux_client::Method;
use serde_json::Value;

use crate::args::Parsed;
use crate::daemon::{DaemonClient, Transport};

pub const DEFAULT_FILE: &str = "natamux-support-bundle.json";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SupportCommand {
    pub out: PathBuf,
}

pub fn parse(words: &[String], values: &Parsed) -> Result<SupportCommand, String> {
    if let Some(extra) = words.first() {
        return Err(format!("Unexpected argument: {extra}"));
    }
    let out = values.text("out").unwrap_or(DEFAULT_FILE);
    Ok(SupportCommand {
        out: PathBuf::from(out),
    })
}

/// A new file with mode 0600 from the first byte (Unix); an old one is replaced.
pub fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Err(error) = std::fs::remove_file(path)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        return Err(error);
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)?.write_all(bytes)
}

/// Gives what to print.
pub async fn run<T: Transport>(
    command: &SupportCommand,
    client: &DaemonClient<T>,
) -> Result<String, String> {
    let body = client
        .call(Method::GET, "/support-bundle", None::<&()>)
        .await?
        .unwrap_or(Value::Null);
    let text = serde_json::to_string_pretty(&body).map_err(|error| error.to_string())?;
    write_private(&command.out, format!("{text}\n").as_bytes())
        .map_err(|error| format!("Cannot write {}: {error}", command.out.display()))?;
    Ok(format!(
        "Saved the support bundle to {}. Read it before you send it.\n",
        command.out.display()
    ))
}
