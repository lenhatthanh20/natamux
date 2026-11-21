//! Reads a theme file of the app (`assets/themes/*.json`) as it is. A key sets the colour field of
//! its name with `_` for `.`; a field the file does not set takes its value from the fallback
//! rows (`fallbacks.tsv`), which hold the final value of every such field per bundled theme.

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, OnceLock};

use anyhow::{Context as _, Result, bail};
use gpui::{FontStyle, FontWeight, HighlightStyle, Hsla, Rgba, WindowBackgroundAppearance, hsla};
use serde::Deserialize;
use serde::de::{Deserializer, MapAccess, Visitor};
use serde_json::Value;

use crate::colors::{AccentColors, Fields, PlayerColor, PlayerColors, StatusColors, ThemeColors};
use crate::syntax::SyntaxTheme;
use crate::theme::{Appearance, Theme, ThemeStyles};

const FALLBACKS: &str = include_str!("fallbacks.tsv");

#[derive(Deserialize)]
struct FamilyFile {
    themes: Vec<ThemeFile>,
}

#[derive(Deserialize)]
struct ThemeFile {
    name: String,
    appearance: AppearanceFile,
    style: StyleFile,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum AppearanceFile {
    Light,
    Dark,
}

#[derive(Deserialize)]
struct StyleFile {
    #[serde(default)]
    players: Vec<PlayerFile>,
    #[serde(default)]
    accents: Vec<Option<String>>,
    #[serde(default)]
    syntax: SyntaxFile,
    /// The colour keys, and any other key.
    #[serde(flatten)]
    colours: HashMap<String, Value>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct PlayerFile {
    cursor: Option<String>,
    background: Option<String>,
    selection: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct SyntaxEntry {
    color: Option<String>,
    background_color: Option<String>,
    font_style: Option<String>,
    font_weight: Option<f32>,
}

/// The syntax entries in the order of the file: the index of an entry is its highlight id.
#[derive(Default)]
struct SyntaxFile(Vec<(String, SyntaxEntry)>);

impl<'de> Deserialize<'de> for SyntaxFile {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct InOrder;

        impl<'de> Visitor<'de> for InOrder {
            type Value = SyntaxFile;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a map of capture names to styles")
            }

            fn visit_map<A: MapAccess<'de>>(
                self,
                mut map: A,
            ) -> std::result::Result<SyntaxFile, A::Error> {
                let mut entries = Vec::new();
                while let Some(entry) = map.next_entry()? {
                    entries.push(entry);
                }
                Ok(SyntaxFile(entries))
            }
        }

        deserializer.deserialize_map(InOrder)
    }
}

impl SyntaxEntry {
    fn style(&self) -> HighlightStyle {
        HighlightStyle {
            color: self.color.as_deref().and_then(parse_color),
            background_color: self.background_color.as_deref().and_then(parse_color),
            font_style: self.font_style.as_deref().and_then(|style| match style {
                "normal" => Some(FontStyle::Normal),
                "italic" => Some(FontStyle::Italic),
                "oblique" => Some(FontStyle::Oblique),
                _ => None,
            }),
            font_weight: self.font_weight.map(FontWeight),
            ..HighlightStyle::default()
        }
    }
}

/// A `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa` colour, as HSL with a positive hue.
pub(crate) fn parse_color(text: &str) -> Option<Hsla> {
    let rgba = Rgba::try_from(text).ok()?;
    let (hue, saturation, lightness) = rgb_to_hsl(rgba.r, rgba.g, rgba.b);
    Some(hsla(hue, saturation, lightness, rgba.a))
}

/// sRGB to HSL, the hue as a turn in `0..1`. The same arithmetic, step for step, as the palette
/// crate that did this before: equal bit for bit on every 8-bit colour, so no colour moved.
fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let (max, min, sep, coeff) = if r > g {
        (r, g, g - b, 0.)
    } else {
        (g, r, b - r, 2.)
    };
    let (max, min, sep, coeff) = if b > max {
        (b, min, r - g, 4.)
    } else {
        (max, min.min(b), sep, coeff)
    };
    let sum = max + min;
    let lightness = sum / 2.;
    if max == min {
        return (0., 0., lightness);
    }
    let d = max - min;
    let saturation = (if sum > 1. { d / (2. - sum) } else { d / sum }).min(1.);
    let degrees = (sep / d + coeff) * 60.;
    let degrees = degrees - (degrees / 360.).floor() * 360.;
    (degrees / 360., saturation, lightness)
}

/// Every theme of one theme file.
pub(crate) fn load_family(bytes: &[u8]) -> Result<Vec<Theme>> {
    let family: FamilyFile = serde_json::from_slice(bytes)?;
    family.themes.into_iter().map(theme_of).collect()
}

fn theme_of(file: ThemeFile) -> Result<Theme> {
    let name = file.name;
    let fallbacks = fallbacks(&name)?;
    let colours: HashMap<String, Option<Hsla>> = file
        .style
        .colours
        .iter()
        .map(|(key, value)| (key.replace('.', "_"), value.as_str().and_then(parse_color)))
        .collect();
    let colors: ThemeColors = fill(&name, &colours, &fallbacks)?;
    let status: StatusColors = fill(&name, &colours, &fallbacks)?;
    let player = file
        .style
        .players
        .iter()
        .map(|player| {
            let color = |text: &Option<String>| {
                text.as_deref()
                    .and_then(parse_color)
                    .with_context(|| format!("{name}: a player lacks a colour"))
            };
            Ok::<_, anyhow::Error>(PlayerColor {
                cursor: color(&player.cursor)?,
                background: color(&player.background)?,
                selection: color(&player.selection)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let accents: Vec<Hsla> = file
        .style
        .accents
        .iter()
        .filter_map(|accent| accent.as_deref().and_then(parse_color))
        .collect();
    if player.is_empty() || accents.is_empty() {
        bail!("{name}: no players or no accents");
    }
    let syntax = SyntaxTheme::new(
        file.style
            .syntax
            .0
            .iter()
            .map(|(capture, entry)| (capture.clone(), entry.style())),
    );
    Ok(Theme {
        id: name.clone(),
        name: name.into(),
        appearance: match file.appearance {
            AppearanceFile::Light => Appearance::Light,
            AppearanceFile::Dark => Appearance::Dark,
        },
        styles: ThemeStyles {
            window_background_appearance: WindowBackgroundAppearance::Opaque,
            accents: AccentColors(Arc::from(accents)),
            colors,
            status,
            player: PlayerColors(player),
            syntax: Arc::new(syntax),
        },
    })
}

/// Each field from the file's colour, else from the fallback rows.
fn fill<T: Fields>(
    theme: &str,
    colours: &HashMap<String, Option<Hsla>>,
    fallbacks: &HashMap<&str, Option<Hsla>>,
) -> Result<T> {
    let mut out = T::default();
    for &field in T::NAMES {
        let value = match colours.get(field) {
            Some(Some(color)) => Some(*color),
            _ => fallbacks
                .get(field)
                .copied()
                .with_context(|| format!("{theme}: no value for {field}"))?,
        };
        if !out.slot(field).is_some_and(|slot| slot.put(value)) {
            bail!("{theme}: {field} needs a colour");
        }
    }
    Ok(out)
}

/// The fallback rows of `theme`: field and value, None for an unset optional field.
fn fallbacks(theme: &str) -> Result<HashMap<&'static str, Option<Hsla>>> {
    type Table = HashMap<&'static str, HashMap<&'static str, Option<Hsla>>>;
    // The rows hold every theme: parse them once, not once per theme.
    static ALL: OnceLock<std::result::Result<Table, String>> = OnceLock::new();
    let all = ALL.get_or_init(|| {
        let mut all = Table::new();
        for (name, field, value) in fallback_rows().map_err(|error| format!("{error:#}"))? {
            all.entry(name).or_default().insert(field, value);
        }
        Ok(all)
    });
    match all {
        Ok(all) => Ok(all.get(theme).cloned().unwrap_or_default()),
        Err(message) => bail!("{message}"),
    }
}

fn fallback_rows() -> Result<Vec<(&'static str, &'static str, Option<Hsla>)>> {
    FALLBACKS
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| {
            let mut parts = line.split('\t');
            let (Some(theme), Some(field), Some(value), None) =
                (parts.next(), parts.next(), parts.next(), parts.next())
            else {
                bail!("bad fallback row: {line}");
            };
            let value = row_value(value).with_context(|| format!("bad fallback row: {line}"))?;
            Ok((theme, field, value))
        })
        .collect()
}

fn row_value(text: &str) -> Result<Option<Hsla>> {
    if text == "none" {
        return Ok(None);
    }
    let parts = text
        .split(' ')
        .map(str::parse::<f32>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let &[h, s, l, a] = parts.as_slice() else {
        bail!("not four numbers");
    };
    Ok(Some(hsla(h, s, l, a)))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn theme_files() -> Vec<ThemeFile> {
        let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../natamux-app/assets/themes");
        let mut out = Vec::new();
        for entry in std::fs::read_dir(folder).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|ext| ext == "json") {
                let family: FamilyFile =
                    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
                out.extend(family.themes);
            }
        }
        out
    }

    #[test]
    fn reads_hex_colours_with_a_positive_hue() {
        assert_eq!(parse_color("#000000ff"), Some(hsla(0., 0., 0., 1.)));
        assert_eq!(parse_color("#ffffff80").map(|c| c.l), Some(1.));
        let blue = parse_color("#0000ff").unwrap();
        assert_eq!((blue.h, blue.s, blue.l), (240. / 360., 1., 0.5));
        let purple = parse_color("#ff00ff").unwrap();
        assert!(purple.h > 0.8 && purple.h < 0.9, "{purple:?}");
        assert_eq!(parse_color("#12345"), None);
        assert_eq!(parse_color("red"), None);
    }

    /// Values that the palette crate gave for these colours (T-0359). A CI run compared the
    /// two on all 16 777 216 colours and found no difference.
    #[test]
    fn hsl_values_are_the_ones_palette_gave() {
        let golden: [(&str, f32, f32, f32); 12] = [
            ("#808080", 0.0, 0.0, 0.5019608),
            ("#1e1e2e", 0.6666667, 0.21052632, 0.14901961),
            ("#7aa2f7", 0.6133333, 0.8865248, 0.7235294),
            ("#bb9af7", 0.72580653, 0.8532111, 0.78627455),
            ("#f7768e", 0.9689923, 0.8896552, 0.71568626),
            ("#9ece6a", 0.24666667, 0.50505054, 0.6117647),
            ("#e0af68", 0.09861111, 0.6593407, 0.6431373),
            ("#0d1117", 0.6, 0.27777776, 0.07058824),
            ("#58a6ff", 0.58882236, 1.0, 0.672549),
            ("#d2a8ff", 0.74712646, 0.9999998, 0.82941175),
            ("#ec4899", 0.9176829, 0.8118812, 0.6039216),
            ("#fefdfc", 0.083333336, 0.5, 0.99215686),
        ];
        for (hex, h, s, l) in golden {
            let color = parse_color(hex).unwrap();
            assert_eq!((color.h, color.s, color.l), (h, s, l), "{hex}");
        }
    }

    #[test]
    fn reads_the_syntax_entries_in_file_order() {
        let text =
            r#"{"zeta": {"color": null}, "alpha": {"font_weight": 700, "font_style": "italic"}}"#;
        let syntax: SyntaxFile = serde_json::from_str(text).unwrap();
        let names: Vec<_> = syntax.0.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["zeta", "alpha"]);
        let style = syntax.0[1].1.style();
        assert_eq!(style.font_weight, Some(FontWeight(700.)));
        assert_eq!(style.font_style, Some(FontStyle::Italic));
        assert_eq!(syntax.0[0].1.style(), HighlightStyle::default());
    }

    #[test]
    fn a_fallback_row_is_only_for_a_field_its_file_does_not_set() {
        let files = theme_files();
        let rows = fallback_rows().unwrap();
        assert!(!rows.is_empty());
        for (theme, field, _) in rows {
            let file = files.iter().find(|file| file.name == theme).unwrap();
            assert!(
                ThemeColors::NAMES.contains(&field) || StatusColors::NAMES.contains(&field),
                "{theme}: {field} is not a field"
            );
            let set = file.style.colours.iter().any(|(key, value)| {
                key.replace('.', "_") == field && value.as_str().and_then(parse_color).is_some()
            });
            assert!(!set, "{theme}: the file sets {field}");
        }
    }

    #[test]
    fn a_field_with_no_value_fails_the_theme() {
        let text = r##"{"themes": [{"name": "Nobody", "appearance": "dark", "style": {
            "players": [{"cursor": "#000", "background": "#000", "selection": "#000"}],
            "accents": ["#000"]}}]}"##;
        let error = load_family(text.as_bytes()).unwrap_err().to_string();
        assert_eq!(error, "Nobody: no value for border");
    }

    #[test]
    fn every_bundled_theme_loads() {
        for file in theme_files() {
            let name = file.name.clone();
            let theme = theme_of(file).unwrap();
            assert_eq!(theme.name, name);
        }
    }
}
