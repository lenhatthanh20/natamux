//! The golden tables of the terminal (docs/plan-mit-license.md 7.3 and 7.6), made from Zed's
//! terminal on CI: the bytes of every key (T-K1) and mouse report (T-M1), the draw list of the
//! `terminal-look` screen (T-P1), the contrast fix (T-C1) and the colours of every theme (T-C2).
//! The own terminal must give the same rows.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gpui::{App, Hsla, Keystroke, MouseButton, Rgba, TestAppContext, VisualTestContext, px, size};
use serde_json::{Value, json};

use super::behaviour_tests::{
    PANE, SET, cell, feed, focus, mods, mouse_down, mouse_move, mouse_up, open, sent_text, settle,
    zed,
};
use crate::app::golden_files;

/// A colour as 8-bit RGBA hex, as a test compares it.
pub(super) fn hex(colour: Hsla) -> String {
    let rgba = Rgba::from(colour);
    let byte = |channel: f32| (channel.clamp(0., 1.) * 255.).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}{:02x}",
        byte(rgba.r),
        byte(rgba.g),
        byte(rgba.b),
        byte(rgba.a)
    )
}

/// The modes a key is pressed in, and how to turn each on. The reset turns all of them off.
const MODES: [(&str, &str); 4] = [
    ("normal", ""),
    ("app-cursor", "\x1b[?1h"),
    ("app-keypad", "\x1b="),
    ("alt-screen", "\x1b[?1049h"),
];
const MODES_OFF: &str = "\x1b[?1l\x1b>\x1b[?1049l";

/// Every key of T-K1, by its gpui name. The test platform has no keypad keys of their own: a
/// keypad key comes as its character, so the digit and operator rows cover it.
fn press_keys() -> Vec<String> {
    let mut keys: Vec<String> = ('a'..='z').chain('0'..='9').map(String::from).collect();
    for key in [
        "`",
        "-",
        "=",
        "[",
        "]",
        "\\",
        ";",
        "'",
        ",",
        ".",
        "/",
        "*",
        "+",
        "space",
        "enter",
        "tab",
        "backspace",
        "escape",
        "up",
        "down",
        "left",
        "right",
        "home",
        "end",
        "pageup",
        "pagedown",
        "insert",
        "delete",
    ] {
        keys.push(key.into());
    }
    keys.extend((1..=20).map(|n| format!("f{n}")));
    keys
}

/// The modifiers of T-K1; Cmd only on macOS.
fn press_modifiers() -> Vec<&'static str> {
    let mut sets = vec![
        "none",
        "shift",
        "ctrl",
        "alt",
        "ctrl-shift",
        "ctrl-alt",
        "shift-alt",
    ];
    if cfg!(target_os = "macos") {
        sets.extend(["cmd", "cmd-shift"]);
    }
    sets
}

fn press(key: &str, modifiers: &str, cx: &mut VisualTestContext) {
    let keystroke = Keystroke {
        modifiers: mods(modifiers),
        key: key.into(),
        key_char: None,
    };
    cx.update(|window, cx| {
        window.dispatch_keystroke(keystroke, cx);
    });
    cx.run_until_parked();
}

/// T-K1: what each key sends in each mode, and whether it went on to the parent.
fn press_rows(cx: &mut TestAppContext) -> Vec<Value> {
    let (host, terminal, cx) = open(cx, PANE, false);
    let mut rows = Vec::new();
    for (mode, on) in MODES {
        feed(&terminal, MODES_OFF, cx);
        feed(&terminal, on, cx);
        sent_text();
        for key in press_keys() {
            for modifiers in press_modifiers() {
                let before = host.read_with(cx, |host, _| host.passed);
                press(&key, modifiers, cx);
                let passed = host.read_with(cx, |host, _| host.passed) > before;
                rows.push(json!([mode, modifiers, key, sent_text(), passed]));
                // A key can open nothing, but keep the focus where the table expects it.
                focus(&terminal, cx);
            }
        }
    }
    // A program turns APP_CURSOR on and no frame is drawn before the next key (6.2.5): the
    // output comes through the screen's writer, as the parser writes it, and nothing draws.
    feed(&terminal, MODES_OFF, cx);
    let screen = zed(&terminal, cx);
    let mut writer = screen.read_with(cx, |screen, _| screen.display_only_writer().unwrap());
    writer.write(b"\x1b[?1h");
    cx.run_until_parked();
    sent_text();
    press("up", "none", cx);
    let no_frame = sent_text();
    rows.push(json!([
        "app-cursor, no frame",
        "none",
        "up",
        no_frame,
        false
    ]));
    rows
}

#[gpui::test]
fn every_key_sends_what_the_golden_holds(cx: &mut TestAppContext) {
    golden_files::check(SET, "press_table", &press_rows(cx));
}

#[gpui::test]
#[ignore]
fn golden_terminal_press_table(cx: &mut TestAppContext) {
    golden_files::write(SET, "press_table", &press_rows(cx));
}

const MOUSE_MODES: [(&str, &str); 3] = [
    ("1000", "\x1b[?1000h"),
    ("1002", "\x1b[?1002h"),
    ("1003", "\x1b[?1003h"),
];
const MOUSE_FORMATS: [(&str, &str); 3] =
    [("x10", ""), ("utf8", "\x1b[?1005h"), ("sgr", "\x1b[?1006h")];
const MOUSE_OFF: &str = "\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1005l\x1b[?1006l";

/// T-M1: press, drag, release and move of each button with each modifier, in each mode and
/// format, at columns below 95, past 95 and past 223.
fn mouse_rows(cx: &mut TestAppContext) -> Vec<Value> {
    let (_, terminal, cx) = open(cx, size(px(1900.), px(300.)), false);
    let mut rows = Vec::new();
    let buttons = [
        ("left", MouseButton::Left),
        ("middle", MouseButton::Middle),
        ("right", MouseButton::Right),
    ];
    for (mode, mode_on) in MOUSE_MODES {
        for (format, format_on) in MOUSE_FORMATS {
            feed(&terminal, MOUSE_OFF, cx);
            feed(&terminal, &format!("{mode_on}{format_on}"), cx);
            sent_text();
            for (name, button) in buttons {
                for modifiers in ["none", "shift", "alt", "ctrl"] {
                    for column in [5, 100, 230] {
                        let at = cell(&terminal, (2, column), cx);
                        let next = cell(&terminal, (3, column + 2), cx);
                        let far = cell(&terminal, (4, column + 4), cx);
                        let held = mods(modifiers);
                        let mut row = |event: &str| {
                            rows.push(json!([
                                mode,
                                format,
                                name,
                                modifiers,
                                column,
                                event,
                                sent_text()
                            ]));
                        };
                        mouse_move(at, None, held, cx);
                        settle(cx);
                        sent_text();
                        mouse_down(at, button, held, 1, cx);
                        settle(cx);
                        row("press");
                        mouse_move(next, Some(button), held, cx);
                        settle(cx);
                        row("drag");
                        mouse_up(next, button, held, 1, cx);
                        settle(cx);
                        row("release");
                        mouse_move(far, None, held, cx);
                        settle(cx);
                        row("move");
                        // A right click outside mouse mode opens the menu: the focus comes back.
                        focus(&terminal, cx);
                        sent_text();
                    }
                }
            }
        }
    }
    rows
}

#[gpui::test]
fn mouse_reports_match_the_golden(cx: &mut TestAppContext) {
    golden_files::check(SET, "mouse_reports", &mouse_rows(cx));
}

#[gpui::test]
#[ignore]
fn golden_terminal_mouse_reports(cx: &mut TestAppContext) {
    golden_files::write(SET, "mouse_reports", &mouse_rows(cx));
}

/// The 17 themes the app offers.
fn themes() -> Vec<&'static str> {
    use crate::app::theme_catalog::ThemeKind;
    let mut names = ThemeKind::Light.themes().to_vec();
    names.extend(ThemeKind::Dark.themes());
    names
}

fn theme(name: &str, cx: &App) -> std::sync::Arc<theme::Theme> {
    theme::ThemeRegistry::global(cx).get(name).unwrap()
}

/// The 16 named colours of a theme's terminal, in ANSI order.
fn ansi(theme: &theme::Theme) -> [(&'static str, Hsla); 16] {
    let c = theme.colors();
    [
        ("black", c.terminal_ansi_black),
        ("red", c.terminal_ansi_red),
        ("green", c.terminal_ansi_green),
        ("yellow", c.terminal_ansi_yellow),
        ("blue", c.terminal_ansi_blue),
        ("magenta", c.terminal_ansi_magenta),
        ("cyan", c.terminal_ansi_cyan),
        ("white", c.terminal_ansi_white),
        ("bright black", c.terminal_ansi_bright_black),
        ("bright red", c.terminal_ansi_bright_red),
        ("bright green", c.terminal_ansi_bright_green),
        ("bright yellow", c.terminal_ansi_bright_yellow),
        ("bright blue", c.terminal_ansi_bright_blue),
        ("bright magenta", c.terminal_ansi_bright_magenta),
        ("bright cyan", c.terminal_ansi_bright_cyan),
        ("bright white", c.terminal_ansi_bright_white),
    ]
}

/// A small fixed random sequence, so the pairs are the same on every run and OS.
struct Seeded(u64);

impl Seeded {
    fn byte(&mut self) -> u8 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 56) as u8
    }

    fn colour(&mut self, alpha: bool) -> Hsla {
        let mut channel = || f32::from(self.byte()) / 255.;
        let (r, g, b) = (channel(), channel(), channel());
        let a = if alpha { channel() } else { 1. };
        Hsla::from(Rgba { r, g, b, a })
    }
}

/// T-C1: the APCA 45 fix of a foreground on a background: the 16 named colours of each theme
/// on its terminal background, then 20,000 random pairs.
fn contrast_rows(cx: &mut TestAppContext) -> Vec<Value> {
    crate::app::test_views::init(cx);
    let mut rows = Vec::new();
    cx.update(|cx| {
        for name in themes() {
            let theme = theme(name, cx);
            let background = theme.colors().terminal_background;
            for (colour, foreground) in ansi(&theme) {
                let fixed = ui::utils::ensure_minimum_contrast(foreground, background, 45.);
                rows.push(json!([
                    name,
                    colour,
                    hex(foreground),
                    hex(background),
                    hex(fixed)
                ]));
            }
        }
    });
    let mut seeded = Seeded(0x004e_6174_616d_7578);
    for i in 0..20_000 {
        let foreground = seeded.colour(i % 10 == 0);
        let background = seeded.colour(false);
        let fixed = ui::utils::ensure_minimum_contrast(foreground, background, 45.);
        rows.push(json!([
            "random",
            i,
            hex(foreground),
            hex(background),
            hex(fixed)
        ]));
    }
    rows
}

#[gpui::test]
fn the_contrast_fix_matches_the_golden(cx: &mut TestAppContext) {
    golden_files::check(SET, "contrast", &contrast_rows(cx));
}

#[gpui::test]
#[ignore]
fn golden_terminal_contrast(cx: &mut TestAppContext) {
    golden_files::write(SET, "contrast", &contrast_rows(cx));
}

#[test]
fn the_seeded_pairs_are_fixed() {
    let mut seeded = Seeded(0x004e_6174_616d_7578);
    let first: Vec<u8> = (0..4).map(|_| seeded.byte()).collect();
    let mut again = Seeded(0x004e_6174_616d_7578);
    assert_eq!(first, (0..4).map(|_| again.byte()).collect::<Vec<_>>());
    assert_ne!(first, [first[0]; 4]);
}

/// The text style the terminal draws with at 13 px: Zed's mono font, ligatures off.
fn text_style(cx: &App) -> gpui::TextStyle {
    use ui::ActiveTheme;
    gpui::TextStyle {
        font_family: ".ZedMono".into(),
        font_features: gpui::FontFeatures::disable_ligatures(),
        font_size: px(13.).into(),
        color: cx.theme().colors().terminal_foreground,
        ..gpui::TextStyle::default()
    }
}

/// A rect's fields are private, so its debug text is kept, with each `Hsla { .. }` written as
/// 8-bit RGBA like every other colour of the tables.
fn hex_colours(text: &str) -> String {
    const START: &str = "Hsla { h: ";
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find(START) {
        out.push_str(&rest[..at]);
        let end = rest[at..].find('}').unwrap() + at;
        let numbers: Vec<f32> = rest[at + START.len() - 3..end]
            .split(", ")
            .map(|part| part.trim().rsplit(' ').next().unwrap().parse().unwrap())
            .collect();
        let [h, s, l, a] = numbers[..] else {
            panic!("not a colour: {}", &rest[at..=end]);
        };
        out.push_str(&hex(Hsla { h, s, l, a }));
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

#[test]
fn writes_the_colours_of_a_rect_as_hex() {
    assert_eq!(
        hex_colours("R { n: 2, color: Hsla { h: 0.0, s: 0.0, l: 1.0, a: 1.0 } }"),
        "R { n: 2, color: #ffffffff }"
    );
}

/// T-P1: the draw list of the `terminal-look` screen: text runs, background rects and block
/// rects, from the screen's cells as the last frame copied them.
fn draw_rows(cx: &mut TestAppContext) -> Vec<Value> {
    let (_, terminal, cx) = open(cx, size(px(1000.), px(500.)), false);
    feed(&terminal, &crate::app::shots_terminal::look(), cx);
    let screen = zed(&terminal, cx);
    let cells = screen.read_with(cx, |screen, _| screen.last_content().cells.clone());
    let mut rows = Vec::new();
    cx.update(|_, cx| {
        let style = text_style(cx);
        let (rects, runs, blocks) = terminal_view::terminal_element::TerminalElement::layout_grid(
            cells.into_iter(),
            0,
            &style,
            None,
            45.,
            cx,
        );
        for run in &runs {
            let style = &run.style;
            rows.push(json!([
                "run",
                format!("{:?}", run.start_point),
                run.text,
                run.cell_count,
                hex(style.color),
                style.font.weight.0,
                format!("{:?}", style.font.style),
                style.background_color.map(hex),
                style.underline.map(|line| (
                    f32::from(line.thickness),
                    line.color.map(hex),
                    line.wavy
                )),
                style
                    .strikethrough
                    .map(|line| (f32::from(line.thickness), line.color.map(hex))),
                format!("{:?}", run.font_size)
            ]));
        }
        for rect in &rects {
            rows.push(json!(["rect", hex_colours(&format!("{rect:?}"))]));
        }
        for block in &blocks {
            rows.push(json!(["block", hex_colours(&format!("{block:?}"))]));
        }
    });
    rows
}

#[gpui::test]
fn the_draw_list_matches_the_golden(cx: &mut TestAppContext) {
    golden_files::check(SET, "draw_list", &draw_rows(cx));
}

#[gpui::test]
#[ignore]
fn golden_terminal_draw_list(cx: &mut TestAppContext) {
    golden_files::write(SET, "draw_list", &draw_rows(cx));
}

/// T-C2: the colour of every named colour, all 256 indexed ones and a sample of exact ones, in
/// each theme.
fn colour_rows(cx: &mut TestAppContext) -> Vec<Value> {
    use terminal::{Color, NamedColor};
    crate::app::test_views::init(cx);
    let named = [
        NamedColor::Black,
        NamedColor::Red,
        NamedColor::Green,
        NamedColor::Yellow,
        NamedColor::Blue,
        NamedColor::Magenta,
        NamedColor::Cyan,
        NamedColor::White,
        NamedColor::BrightBlack,
        NamedColor::BrightRed,
        NamedColor::BrightGreen,
        NamedColor::BrightYellow,
        NamedColor::BrightBlue,
        NamedColor::BrightMagenta,
        NamedColor::BrightCyan,
        NamedColor::BrightWhite,
        NamedColor::Foreground,
        NamedColor::Background,
        NamedColor::Cursor,
        NamedColor::DimBlack,
        NamedColor::DimRed,
        NamedColor::DimGreen,
        NamedColor::DimYellow,
        NamedColor::DimBlue,
        NamedColor::DimMagenta,
        NamedColor::DimCyan,
        NamedColor::DimWhite,
        NamedColor::BrightForeground,
        NamedColor::DimForeground,
    ];
    let mut colours: Vec<Color> = named.into_iter().map(Color::Named).collect();
    colours.extend((0..=255).map(Color::Indexed));
    for (r, g, b) in [(0, 0, 0), (255, 255, 255), (18, 52, 86), (250, 128, 3)] {
        colours.push(Color::Spec(terminal::Rgb { r, g, b }));
    }
    let mut rows = Vec::new();
    cx.update(|cx| {
        for name in themes() {
            let theme = theme(name, cx);
            for colour in &colours {
                let drawn = terminal_view::terminal_element::convert_color(colour, &theme);
                rows.push(json!([name, format!("{colour:?}"), hex(drawn)]));
            }
        }
    });
    rows
}

#[gpui::test]
fn the_colours_match_the_golden(cx: &mut TestAppContext) {
    golden_files::check(SET, "colours", &colour_rows(cx));
}

#[gpui::test]
#[ignore]
fn golden_terminal_colours(cx: &mut TestAppContext) {
    golden_files::write(SET, "colours", &colour_rows(cx));
}
