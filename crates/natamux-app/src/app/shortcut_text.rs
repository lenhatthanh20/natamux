//! How the registry's shortcuts are written, read and dispatched: `Mod+Shift+Enter` as gpui
//! writes a keystroke, as people read it, and which ones a terminal leaves to the app.

struct Parsed<'a> {
    modifiers: Vec<&'a str>,
    key: &'a str,
}

fn parse(shortcut: &str) -> Parsed<'_> {
    // In `Mod++` the key is the plus sign.
    let (head, key) = match shortcut.strip_suffix("++") {
        Some(head) => (head, "+"),
        None => shortcut.rsplit_once('+').unwrap_or(("", shortcut)),
    };
    Parsed {
        modifiers: head.split('+').filter(|part| !part.is_empty()).collect(),
        key,
    }
}

/// `Mod+Shift+Enter` as gpui writes a keystroke: `cmd-shift-enter` on macOS.
pub fn keystroke(shortcut: &str, mac: bool) -> String {
    let parsed = parse(shortcut);
    let mut out: Vec<String> = parsed
        .modifiers
        .iter()
        .map(|modifier| match *modifier {
            "Mod" if mac => "cmd".to_string(),
            "Mod" => "ctrl".to_string(),
            other => other.to_lowercase(),
        })
        .collect();
    let key = match parsed.key {
        "ArrowUp" => "up".to_string(),
        "ArrowDown" => "down".to_string(),
        "ArrowLeft" => "left".to_string(),
        "ArrowRight" => "right".to_string(),
        // The Menu key of Windows keyboards (VK_APPS).
        "ContextMenu" => "menu".to_string(),
        other => other.to_lowercase(),
    };
    out.push(key);
    out.join("-")
}

// Kbd and tooltips draw keys with `components::input_rules::kbd_text`.
#[cfg_attr(not(test), allow(dead_code))]
/// The key as people read it: `⇧⌘↵` on macOS, `Ctrl+Shift+Enter` elsewhere.
pub fn key_text(shortcut: &str, mac: bool) -> String {
    let parsed = parse(shortcut);
    let arrow = match parsed.key {
        "ArrowUp" => Some("↑"),
        "ArrowDown" => Some("↓"),
        "ArrowLeft" => Some("←"),
        "ArrowRight" => Some("→"),
        _ => None,
    };
    let key = match (parsed.key, arrow) {
        (_, Some(arrow)) => arrow.to_string(),
        ("Enter", None) if mac => "↵".to_string(),
        ("Escape", None) => "Esc".to_string(),
        (key, None) if key.chars().count() == 1 => key.to_uppercase(),
        (key, None) => key.to_string(),
    };
    if !mac {
        let mut parts: Vec<&str> = parsed
            .modifiers
            .iter()
            .map(|modifier| if *modifier == "Mod" { "Ctrl" } else { modifier })
            .collect();
        parts.push(&key);
        return parts.join("+");
    }
    // Apple's order of the modifier signs.
    let signs = [("Ctrl", "⌃"), ("Alt", "⌥"), ("Shift", "⇧"), ("Mod", "⌘")];
    let mut text: String = signs
        .iter()
        .filter(|(name, _)| parsed.modifiers.contains(name))
        .map(|(_, sign)| *sign)
        .collect();
    text.push_str(&key);
    text
}

/// A key with no modifier but Shift: it types a character (`isSingleKey`).
pub fn is_single_key(shortcut: &str) -> bool {
    let parsed = parse(shortcut);
    let function_key = is_function_key(parsed.key);
    !function_key && parsed.modifiers.iter().all(|modifier| *modifier == "Shift")
}

fn is_function_key(key: &str) -> bool {
    key.len() >= 2
        && key.len() <= 3
        && key.starts_with('F')
        && key[1..].bytes().all(|byte| byte.is_ascii_digit())
}

/// Inside a terminal the shell owns the keys. On a Mac Cmd chords never reach the shell, so they
/// are the app's. Elsewhere only function keys and Ctrl+Shift chords are
/// (`reachesAppFromTerminal`).
pub fn reaches_app_from_terminal(shortcut: &str, mac: bool) -> bool {
    let parsed = parse(shortcut);
    if is_function_key(parsed.key) {
        return true;
    }
    let has = |name: &str| parsed.modifiers.contains(&name);
    if mac && has("Mod") {
        return true;
    }
    (has("Mod") || has("Ctrl")) && has("Shift")
}
