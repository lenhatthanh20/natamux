//! The themes against the theme value dump of the old code (docs/plan-mit-license.md S9.1,
//! `crates/natamux-app/tests/golden/theme`): every value of the 17 themes, light and dark, must
//! be the same. Then the globals: which theme shows, and when it changes.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::{AssetSource, HighlightStyle, Hsla, SharedString, TestAppContext, WindowAppearance, px};
use natamux_theme::{
    ActiveTheme, Appearance, GlobalTheme, SystemAppearance, Theme, ThemeMode, ThemeRegistry,
    reload_theme, set_appearance, setup_ui_font,
};
use serde_json::{Value, json};

/// The app's assets folder, read from disk.
struct AppAssets;

fn assets_folder() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../natamux-app/assets")
}

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        Ok(std::fs::read(assets_folder().join(path)).ok().map(Cow::Owned))
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(assets_folder().join(path))? {
            let name = entry?.file_name().to_string_lossy().into_owned();
            out.push(format!("{path}{name}").into());
        }
        Ok(out)
    }
}

/// The 17 themes of the app's menu, light then dark (`app/theme_catalog.rs`).
const THEMES: [&str; 17] = [
    "Natamux Light",
    "Ayu Light",
    "Catppuccin Latte",
    "GitHub Light",
    "Gruvbox Light",
    "Rosé Pine Dawn",
    "Solarized Light",
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

fn slug(name: &str) -> String {
    name.to_lowercase().replace(' ', "-").replace('é', "e")
}

/// A float as its shortest text, as the dump writes it.
fn num(value: f32) -> Value {
    json!(value.to_string().parse::<f64>().unwrap())
}

fn hsla(color: Hsla) -> Value {
    json!([num(color.h), num(color.s), num(color.l), num(color.a)])
}

fn style_row(theme: &str, index: usize, name: &str, style: &HighlightStyle) -> Value {
    json!({
        "theme": theme, "group": "syntax", "index": index, "name": name,
        "color": style.color.map(hsla),
        "font_weight": style.font_weight.map(|weight| num(weight.0)),
        "font_style": style.font_style.map(|font_style| format!("{font_style:?}")),
        "background_color": style.background_color.map(hsla),
        "underline": style.underline.map(|underline| format!("{underline:?}")),
        "strikethrough": style.strikethrough.map(|line| format!("{line:?}")),
        "fade_out": style.fade_out.map(num),
    })
}

/// The rows of the dump, made from the own theme.
fn theme_rows(theme: &Theme) -> Vec<Value> {
    let name = theme.name.as_ref();
    let mut rows = vec![json!({
        "theme": name, "group": "meta",
        "appearance": format!("{:?}", theme.appearance),
        "window_background": format!("{:?}", theme.window_background_appearance()),
    })];
    for (group, fields) in [
        ("colors", theme.colors().fields()),
        ("status", theme.status().fields()),
    ] {
        for (field, value) in fields {
            rows.push(json!({"theme": name, "group": group, "field": field, "value": value.map(hsla)}));
        }
    }
    for (i, color) in theme.accents().0.iter().enumerate() {
        rows.push(json!({"theme": name, "group": "accents", "index": i, "value": hsla(*color)}));
    }
    for (i, player) in theme.players().0.iter().enumerate() {
        rows.push(json!({
            "theme": name, "group": "players", "index": i,
            "cursor": hsla(player.cursor),
            "background": hsla(player.background),
            "selection": hsla(player.selection),
        }));
    }
    let syntax = theme.syntax();
    let mut index = 0usize;
    while let Some(style) = syntax.get(index) {
        let capture = syntax.get_capture_name(index).unwrap();
        rows.push(style_row(name, index, capture, style));
        index += 1;
    }
    rows
}

fn golden(slug: &str) -> Vec<Value> {
    let os = if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    };
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../natamux-app/tests/golden/theme")
        .join(os)
        .join(format!("{slug}.json"));
    serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap()
}

#[test]
fn every_theme_holds_every_value_of_the_dump() {
    let registry = ThemeRegistry::from_assets(&AppAssets).unwrap();
    assert_eq!(registry.list_names().len(), THEMES.len());
    let mut wrong = Vec::new();
    for name in THEMES {
        let theme = registry.get(name).unwrap();
        let own = theme_rows(&theme);
        let want = golden(&slug(name));
        if own.len() != want.len() {
            wrong.push(format!("{name}: {} rows, the dump has {}", own.len(), want.len()));
        }
        for (own, want) in own.iter().zip(&want) {
            if own != want {
                wrong.push(format!("{name}:\n  own  {own}\n  dump {want}"));
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "{} rows differ:\n{}",
        wrong.len(),
        wrong[..wrong.len().min(20)].join("\n")
    );
}

#[test]
fn both_looks_have_their_themes() {
    let registry = ThemeRegistry::from_assets(&AppAssets).unwrap();
    for (i, name) in THEMES.iter().enumerate() {
        let look = registry.get(name).unwrap().appearance;
        assert_eq!(look.is_light(), i < 7, "{name}");
    }
    assert!(registry.get("One Dark").is_err());
}

fn shown(cx: &mut TestAppContext) -> Arc<Theme> {
    cx.update(|cx| cx.theme().clone())
}

#[gpui::test]
fn the_test_init_shows_natamux_dark(cx: &mut TestAppContext) {
    cx.update(|cx| natamux_theme::init_for_tests(AppAssets, cx).unwrap());
    assert_eq!(shown(cx).name, "Natamux Dark");
    let size = cx.update(|cx| natamux_theme::theme_settings(cx).ui_font_size(cx));
    assert_eq!(size, px(16.));
}

#[gpui::test]
fn the_system_look_picks_the_theme_in_system_mode(cx: &mut TestAppContext) {
    cx.update(|cx| {
        natamux_theme::init(AppAssets, cx).unwrap();
        set_appearance(cx, |settings| {
            settings.mode = ThemeMode::System;
            settings.light_theme = "Ayu Light".into();
            settings.dark_theme = "Nord".into();
        });
        *SystemAppearance::global_mut(cx) = SystemAppearance(WindowAppearance::Dark.into());
        reload_theme(cx);
    });
    assert_eq!(shown(cx).name, "Nord");
    cx.update(|cx| {
        *SystemAppearance::global_mut(cx) = SystemAppearance(Appearance::Light);
        reload_theme(cx);
    });
    assert_eq!(shown(cx).name, "Ayu Light");
    cx.update(|cx| set_appearance(cx, |settings| settings.mode = ThemeMode::Dark));
    assert_eq!(shown(cx).name, "Nord");
}

#[gpui::test]
fn the_theme_changes_only_with_its_name(cx: &mut TestAppContext) {
    cx.update(|cx| natamux_theme::init_for_tests(AppAssets, cx).unwrap());
    let before = shown(cx);
    let notified = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count = notified.clone();
    let _watch = cx.update(|cx| {
        cx.observe_global::<GlobalTheme>(move |_| {
            count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        })
    });
    cx.update(|cx| set_appearance(cx, |settings| settings.light_theme = "Ayu Light".into()));
    assert!(Arc::ptr_eq(&before, &shown(cx)));
    assert_eq!(notified.load(std::sync::atomic::Ordering::SeqCst), 0);
    cx.update(|cx| set_appearance(cx, |settings| settings.dark_theme = "Dracula".into()));
    assert_eq!(shown(cx).name, "Dracula");
    assert_eq!(notified.load(std::sync::atomic::Ordering::SeqCst), 1);
    cx.update(|cx| set_appearance(cx, |settings| settings.dark_theme = "No Such".into()));
    assert_eq!(shown(cx).name, "Natamux Dark");
}

#[gpui::test]
fn overrides_change_the_shown_theme_only(cx: &mut TestAppContext) {
    cx.update(|cx| natamux_theme::init_for_tests(AppAssets, cx).unwrap());
    let clear = Hsla::transparent_black();
    cx.update(|cx| {
        set_appearance(cx, |settings| {
            settings.color_overrides = vec![("scrollbar_track_border".into(), clear)];
        })
    });
    assert_eq!(shown(cx).colors().scrollbar_track_border, clear);
    let stored = cx.update(|cx| ThemeRegistry::global(cx).get("Natamux Dark").unwrap());
    assert_ne!(stored.colors().scrollbar_track_border, clear);
    assert_eq!(shown(cx).colors().border, stored.colors().border);
}

#[gpui::test]
fn the_ui_font_sets_the_rem_size(cx: &mut TestAppContext) {
    cx.update(|cx| {
        natamux_theme::init_for_tests(AppAssets, cx).unwrap();
        set_appearance(cx, |settings| {
            *settings = natamux_theme::ThemeSettings {
                mode: settings.mode,
                ..natamux_theme::ThemeSettings::at_zoom(1.077)
            }
        });
    });
    let window = cx.add_empty_window();
    let (font, rem) = window.update(|window, cx| {
        let font = setup_ui_font(window, cx);
        (font, window.rem_size())
    });
    assert_eq!(font.family, ".ZedSans");
    assert_eq!(rem, px(17.2));
}
