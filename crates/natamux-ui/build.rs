//! Sets `macos_sdk_26_or_later` when the macOS SDK of this build is version 26 or newer, for the
//! title bar gutter in `utils.rs` (plan D21).

use std::process::Command;

fn main() {
    println!("cargo::rustc-check-cfg=cfg(macos_sdk_26_or_later)");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let Ok(output) = Command::new("xcrun")
        .args(["--sdk", "macosx", "--show-sdk-version"])
        .output()
    else {
        return;
    };
    let version = String::from_utf8_lossy(&output.stdout);
    let major = version
        .trim()
        .split('.')
        .next()
        .and_then(|major| major.parse::<u32>().ok());
    if major.is_some_and(|major| major >= 26) {
        println!("cargo::rustc-cfg=macos_sdk_26_or_later");
    }
}
