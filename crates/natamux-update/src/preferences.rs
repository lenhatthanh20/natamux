//! The automatic check flag in the app's existing settings file.

use serde_json::{Value, json};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

pub fn auto_update(text: &str) -> bool {
    serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|value| value.get("autoUpdate").and_then(Value::as_bool))
        == Some(true)
}

pub fn set_auto_update(text: &str, enabled: bool) -> Result<String, String> {
    let mut value: Value = serde_json::from_str(text).unwrap_or_else(|_| json!({}));
    let object = value
        .as_object_mut()
        .ok_or("settings.json must hold a JSON object")?;
    object.insert("autoUpdate".into(), Value::Bool(enabled));
    serde_json::to_string_pretty(&value).map_err(|error| error.to_string())
}

pub fn load(file: &Path) -> io::Result<bool> {
    match fs::read_to_string(file) {
        Ok(text) => Ok(auto_update(&text)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

pub fn save(file: &Path, enabled: bool) -> io::Result<()> {
    let text = match fs::read_to_string(file) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => "{}".into(),
        Err(error) => return Err(error),
    };
    let saved = set_auto_update(&text, enabled).map_err(io::Error::other)?;
    let parent = file
        .parent()
        .ok_or_else(|| io::Error::other("No settings folder"))?;
    fs::create_dir_all(parent)?;
    let mut part = file.as_os_str().to_owned();
    part.push(format!(".{}.part", std::process::id()));
    let part = std::path::PathBuf::from(part);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let result = (|| {
        let mut output = options.open(&part)?;
        output.write_all(saved.as_bytes())?;
        output.sync_all()?;
        fs::rename(&part, file)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&part);
    }
    result
}
