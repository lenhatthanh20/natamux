//! The one look of every button (T-0168): a grey Zed button. `Outlined` for an action, `Subtle`
//! for a quiet one (Cancel, Retry, a text link), and the icon or key hint before the label. No
//! button is `Tinted`, `Filled` or `Transparent`: Zed's hover only darkens a tint by 5%, fades a
//! fill by half and gives a transparent button no background, so none of them shows a grey hover.

use gpui::{KeybindingKeystroke, Keystroke};
use ui::{KeyBinding, KeybindingPosition, prelude::*};

use super::icons::icon_path;
use super::spinner::Spinner;

/// A button for an action: grey with a border, its key hint before the label.
pub fn action_button(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    Button::new(id, label)
        .style(ButtonStyle::Outlined)
        .key_binding_position(KeybindingPosition::Start)
}

/// A quiet button: no border, a grey background on hover only.
pub fn quiet_button(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    Button::new(id, label)
        .style(ButtonStyle::Subtle)
        .key_binding_position(KeybindingPosition::Start)
}

/// The busy state of a button (docs/ui-spec.md, Button states): from the click until the answer,
/// the 8-dash spinner takes the place of its start icon and the button is off; the label and the
/// key hint stay. Only the button whose work runs is busy; the others are only off.
pub fn busy(button: Button, running: bool) -> Button {
    button.spinner(running.then(|| Spinner::new(IconSize::Small)))
}

/// `busy` for a button built from `ButtonLike`: the spinner in place of `icon`, of the same size.
/// The caller turns the button off.
pub fn busy_icon(icon: impl IntoElement, size: IconSize, running: bool) -> AnyElement {
    if running {
        Spinner::new(size).into_any_element()
    } else {
        icon.into_any_element()
    }
}

/// The icon before a button's label, by its Lucide name (Zed's twin where there is one).
pub fn button_icon(name: &str) -> Icon {
    Icon::from_path(icon_path(name))
        .size(IconSize::Small)
        .color(Color::Muted)
}

/// The kit's key hint for keys of the command registry ("mod-enter", "a"), the size of a Zed modal
/// footer's hint. None for keys gpui cannot read.
pub fn key_hint(keys: &str, disabled: bool) -> Option<KeyBinding> {
    let strokes = keys
        .split_whitespace()
        .map(|stroke| {
            Keystroke::parse(&gpui_stroke(stroke))
                .ok()
                .map(KeybindingKeystroke::from_keystroke)
        })
        .collect::<Option<Vec<_>>>()?;
    if strokes.is_empty() {
        return None;
    }
    Some(
        KeyBinding::from_keystrokes(strokes.into())
            .size(rems_from_px(12_f32))
            .disabled(disabled),
    )
}

/// One stroke in gpui's words: `mod` is `secondary` (Cmd on macOS, Ctrl elsewhere).
fn gpui_stroke(stroke: &str) -> String {
    let mut parts: Vec<&str> = stroke.split('-').collect();
    let key = parts.pop().unwrap_or_default();
    let mut words: Vec<&str> = parts
        .into_iter()
        .map(|part| match part.to_ascii_lowercase().as_str() {
            "mod" | "secondary" => "secondary",
            "control" | "ctrl" => "ctrl",
            "platform" | "super" | "cmd" => "cmd",
            "alt" => "alt",
            _ => "shift",
        })
        .collect();
    let key = match key.to_ascii_lowercase().as_str() {
        "esc" => "escape",
        "return" => "enter",
        _ => key,
    };
    words.push(key);
    words.join("-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_running_button_is_busy_and_off() {
        let running = busy(
            action_button("save", "Save").start_icon(button_icon("check")),
            true,
        );
        assert!(running.is_busy());
        assert!(running.is_off());
        // A later `.disabled(false)` does not turn it back on.
        assert!(running.disabled(false).is_off());
    }

    #[test]
    fn a_button_that_does_not_run_keeps_its_state() {
        let idle = busy(action_button("save", "Save"), false);
        assert!(!idle.is_busy());
        assert!(!idle.is_off());
        let off = busy(action_button("save", "Save").disabled(true), false);
        assert!(!off.is_busy());
        assert!(off.is_off());
    }

    #[test]
    fn mod_is_the_secondary_modifier_of_gpui() {
        assert_eq!(gpui_stroke("mod-enter"), "secondary-enter");
        assert_eq!(gpui_stroke("mod-shift-g"), "secondary-shift-g");
        assert_eq!(gpui_stroke("a"), "a");
        assert_eq!(gpui_stroke("esc"), "escape");
        assert_eq!(gpui_stroke("ctrl-return"), "ctrl-enter");
    }

    #[test]
    fn a_key_hint_reads_the_app_keys() {
        assert!(key_hint("mod-enter", false).is_some());
        assert!(key_hint("escape", true).is_some());
        assert!(key_hint("a", false).is_some());
        assert!(key_hint("", false).is_none());
    }

    /// The looks with no grey hover, written in two parts so this file does not match itself.
    const BANNED: [(&str, &str); 4] = [
        ("ButtonStyle::", "Tinted"),
        ("ButtonStyle::", "Filled"),
        ("ButtonStyle::", "Transparent"),
        ("TintColor", "::"),
    ];

    fn rust_files(dir: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                rust_files(&path, files);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                files.push(path);
            }
        }
    }

    /// A second spinner (Zed's turning circle), written in two parts so this file does not match
    /// itself. `busy` and `busy_icon` are the one way to show a button at work.
    const OTHER_SPINNERS: [(&str, &str); 2] = [("LoadCir", "cle"), ("\"load", "er")];

    #[test]
    fn buttons_show_work_with_the_one_busy_spinner() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        rust_files(&src, &mut files);
        assert!(!files.is_empty());
        let mut found = Vec::new();
        for file in files {
            let text = std::fs::read_to_string(&file).unwrap_or_default();
            for (line_no, line) in text.lines().enumerate() {
                for (head, tail) in OTHER_SPINNERS {
                    if line.contains(&format!("{head}{tail}")) {
                        found.push(format!("{}:{}", file.display(), line_no + 1));
                    }
                }
            }
        }
        assert!(found.is_empty(), "use busy() or busy_icon(): {found:?}");
    }

    #[test]
    fn no_button_is_tinted_filled_or_transparent() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        rust_files(&src, &mut files);
        assert!(!files.is_empty());
        let mut found = Vec::new();
        for file in files {
            let text = std::fs::read_to_string(&file).unwrap_or_default();
            for (line_no, line) in text.lines().enumerate() {
                for (head, tail) in BANNED {
                    if line.contains(&format!("{head}{tail}")) {
                        found.push(format!("{}:{}", file.display(), line_no + 1));
                    }
                }
            }
        }
        assert!(found.is_empty(), "grey buttons only: {found:?}");
    }
}
