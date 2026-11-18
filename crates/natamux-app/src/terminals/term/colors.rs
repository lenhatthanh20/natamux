//! The colour of a terminal cell (docs/plan-mit-license.md 6.2.6): the 16 named colours and the
//! terminal's own colours come from the theme, the 256 table from xterm's cube and gray ramp, and
//! an exact colour stays as the program sent it.

use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};
use gpui::{Hsla, Rgba};
use theme::Theme;

/// The colour a cell draws with in this theme.
pub fn convert(color: &Color, theme: &Theme) -> Hsla {
    match color {
        Color::Named(named) => named_colour(*named, theme),
        Color::Indexed(index) => indexed(*index, theme),
        Color::Spec(rgb) => exact(*rgb),
    }
}

fn named_colour(named: NamedColor, theme: &Theme) -> Hsla {
    let c = theme.colors();
    match named {
        NamedColor::Black => c.terminal_ansi_black,
        NamedColor::Red => c.terminal_ansi_red,
        NamedColor::Green => c.terminal_ansi_green,
        NamedColor::Yellow => c.terminal_ansi_yellow,
        NamedColor::Blue => c.terminal_ansi_blue,
        NamedColor::Magenta => c.terminal_ansi_magenta,
        NamedColor::Cyan => c.terminal_ansi_cyan,
        NamedColor::White => c.terminal_ansi_white,
        NamedColor::BrightBlack => c.terminal_ansi_bright_black,
        NamedColor::BrightRed => c.terminal_ansi_bright_red,
        NamedColor::BrightGreen => c.terminal_ansi_bright_green,
        NamedColor::BrightYellow => c.terminal_ansi_bright_yellow,
        NamedColor::BrightBlue => c.terminal_ansi_bright_blue,
        NamedColor::BrightMagenta => c.terminal_ansi_bright_magenta,
        NamedColor::BrightCyan => c.terminal_ansi_bright_cyan,
        NamedColor::BrightWhite => c.terminal_ansi_bright_white,
        NamedColor::Foreground => c.terminal_foreground,
        NamedColor::Background => c.terminal_ansi_background,
        NamedColor::DimBlack => c.terminal_ansi_dim_black,
        NamedColor::DimRed => c.terminal_ansi_dim_red,
        NamedColor::DimGreen => c.terminal_ansi_dim_green,
        NamedColor::DimYellow => c.terminal_ansi_dim_yellow,
        NamedColor::DimBlue => c.terminal_ansi_dim_blue,
        NamedColor::DimMagenta => c.terminal_ansi_dim_magenta,
        NamedColor::DimCyan => c.terminal_ansi_dim_cyan,
        NamedColor::DimWhite => c.terminal_ansi_dim_white,
        NamedColor::BrightForeground => c.terminal_bright_foreground,
        NamedColor::DimForeground => c.terminal_dim_foreground,
        NamedColor::Cursor => theme.players().local().cursor,
    }
}

/// The 256 table: 0-15 are the named colours, then a 6 x 6 x 6 cube and a ramp of 24 grays.
fn indexed(index: u8, theme: &Theme) -> Hsla {
    const NAMED: [NamedColor; 16] = [
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
    ];
    match index {
        0..=15 => named_colour(NAMED[usize::from(index)], theme),
        16..=231 => {
            let i = index - 16;
            let step = |v: u8| if v == 0 { 0 } else { v * 40 + 55 };
            exact(Rgb {
                r: step(i / 36),
                g: step(i / 6 % 6),
                b: step(i % 6),
            })
        }
        232..=255 => {
            let v = (index - 232) * 10 + 8;
            exact(Rgb { r: v, g: v, b: v })
        }
    }
}

fn exact(rgb: Rgb) -> Hsla {
    let channel = |v: u8| f32::from(v) / 255.;
    Hsla::from(Rgba {
        r: channel(rgb.r),
        g: channel(rgb.g),
        b: channel(rgb.b),
        a: 1.,
    })
}
