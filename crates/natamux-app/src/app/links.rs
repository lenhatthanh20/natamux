//! Links the app opens outside itself. One check, so a file or an agent cannot start a program
//! through another scheme.

use gpui::App;

/// Only web and mail addresses open, so a file cannot start a program through another scheme.
pub fn is_safe_link(href: &str) -> bool {
    let Some((scheme, _)) = href.split_once(':') else {
        return false;
    };
    let mut chars = scheme.chars();
    let valid = chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c));
    valid && ["http", "https", "mailto"].contains(&scheme.to_ascii_lowercase().as_str())
}

/// Opens a safe link in the system handler. Any other link is ignored.
pub fn open_link(url: &str, cx: &App) {
    if is_safe_link(url) {
        cx.open_url(url);
    }
}

#[cfg(test)]
#[path = "links_tests.rs"]
mod tests;
