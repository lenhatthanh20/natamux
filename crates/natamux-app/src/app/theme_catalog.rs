//! The themes the app offers, light or dark, in menu order. Each one is a theme of a file in
//! `assets/themes/`, checked for contrast by `components/theme_contrast.rs`. No gpui, so it can be
//! tested alone.

use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeKind {
    Light,
    Dark,
}

impl ThemeKind {
    /// The theme a missing or unknown name falls back to.
    pub fn default_theme(self) -> &'static str {
        match self {
            ThemeKind::Light => LIGHT[0],
            ThemeKind::Dark => DARK[0],
        }
    }

    pub fn themes(self) -> &'static [&'static str] {
        match self {
            ThemeKind::Light => &LIGHT,
            ThemeKind::Dark => &DARK,
        }
    }
}

const LIGHT: [&str; 7] = [
    "Natamux Light",
    "Ayu Light",
    "Catppuccin Latte",
    "GitHub Light",
    "Gruvbox Light",
    "Rosé Pine Dawn",
    "Solarized Light",
];

const DARK: [&str; 10] = [
    "Natamux Dark",
    "Ayu Dark",
    "Catppuccin Mocha",
    "Dracula",
    "GitHub Dark",
    "Gruvbox Dark",
    "Nord",
    "Rosé Pine",
    "Solarized Dark",
    "Tokyo Night",
];

/// The stored name when the app offers it, else the default of `kind`.
pub fn parse_theme_name(raw: &str, kind: ThemeKind) -> &'static str {
    kind.themes()
        .iter()
        .copied()
        .find(|name| *name == raw)
        .unwrap_or(kind.default_theme())
}

/// The theme of the light look and of the dark look (`lightTheme`, `darkTheme` of `view.json`).
/// The mode (`theme`) picks which one shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThemeNames {
    pub light: &'static str,
    pub dark: &'static str,
}

impl Default for ThemeNames {
    fn default() -> Self {
        ThemeNames {
            light: ThemeKind::Light.default_theme(),
            dark: ThemeKind::Dark.default_theme(),
        }
    }
}

impl ThemeNames {
    /// A name that is missing, not text or not offered is the default.
    pub fn parse(light: Option<&Value>, dark: Option<&Value>) -> Self {
        let name = |value: Option<&Value>, kind| {
            parse_theme_name(value.and_then(Value::as_str).unwrap_or_default(), kind)
        };
        ThemeNames {
            light: name(light, ThemeKind::Light),
            dark: name(dark, ThemeKind::Dark),
        }
    }

    pub fn get(self, kind: ThemeKind) -> &'static str {
        match kind {
            ThemeKind::Light => self.light,
            ThemeKind::Dark => self.dark,
        }
    }

    /// With `name` as the theme of `kind`; a name the app does not offer is the default.
    pub fn with(self, kind: ThemeKind, name: &str) -> Self {
        let name = parse_theme_name(name, kind);
        match kind {
            ThemeKind::Light => ThemeNames {
                light: name,
                ..self
            },
            ThemeKind::Dark => ThemeNames { dark: name, ..self },
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::app::asset_table;

    /// (name, appearance) of every theme in the files.
    fn themes_in_files() -> Vec<(String, String)> {
        let mut out = Vec::new();
        for path in asset_table::paths_with_prefix("themes/").filter(|p| p.ends_with(".json")) {
            let bytes = asset_table::find(path).expect("listed file");
            let family: Value = serde_json::from_slice(bytes).expect("theme file is JSON");
            for theme in family["themes"].as_array().expect("themes") {
                let field = |key: &str| theme[key].as_str().expect(key).to_owned();
                out.push((field("name"), field("appearance")));
            }
        }
        out
    }

    #[test]
    fn every_offered_theme_is_in_a_file_with_its_appearance() {
        let files = themes_in_files();
        for (kind, appearance) in [(ThemeKind::Light, "light"), (ThemeKind::Dark, "dark")] {
            for name in kind.themes() {
                assert!(
                    files.contains(&(name.to_string(), appearance.to_owned())),
                    "{name} is not a {appearance} theme of assets/themes"
                );
            }
        }
    }

    #[test]
    fn every_theme_in_the_files_is_offered() {
        for (name, appearance) in themes_in_files() {
            let kind = if appearance == "light" {
                ThemeKind::Light
            } else {
                ThemeKind::Dark
            };
            assert!(
                kind.themes().contains(&name.as_str()),
                "{name} is not offered"
            );
        }
    }

    #[test]
    fn natamux_comes_first_and_is_the_default() {
        assert_eq!(ThemeKind::Light.default_theme(), "Natamux Light");
        assert_eq!(ThemeKind::Dark.default_theme(), "Natamux Dark");
    }

    #[test]
    fn an_unknown_or_wrong_kind_name_falls_back_to_the_default() {
        assert_eq!(parse_theme_name("Dracula", ThemeKind::Dark), "Dracula");
        assert_eq!(
            parse_theme_name("Dracula", ThemeKind::Light),
            "Natamux Light"
        );
        assert_eq!(parse_theme_name("", ThemeKind::Dark), "Natamux Dark");
        assert_eq!(
            parse_theme_name("One Dark", ThemeKind::Dark),
            "Natamux Dark"
        );
    }

    #[test]
    fn reads_the_stored_names_and_falls_back_for_each_alone() {
        let names = ThemeNames::parse(Some(&json!("GitHub Light")), Some(&json!("Nord")));
        assert_eq!(names.get(ThemeKind::Light), "GitHub Light");
        assert_eq!(names.get(ThemeKind::Dark), "Nord");
        let mixed = ThemeNames::parse(Some(&json!(3)), Some(&json!("Nord")));
        assert_eq!(mixed.light, "Natamux Light");
        assert_eq!(mixed.dark, "Nord");
        assert_eq!(ThemeNames::parse(None, None), ThemeNames::default());
    }

    #[test]
    fn picking_one_look_keeps_the_other() {
        let names = ThemeNames::default().with(ThemeKind::Dark, "Dracula");
        assert_eq!(names.dark, "Dracula");
        assert_eq!(names.light, "Natamux Light");
        let back = names.with(ThemeKind::Dark, "Ayu Light");
        assert_eq!(back.dark, "Natamux Dark");
    }
}
