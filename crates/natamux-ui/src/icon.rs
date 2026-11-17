//! Icons: an SVG of the app's assets drawn in a theme colour, or an image file from disk.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::{
    AbsoluteLength, App, Hsla, IntoElement, Rems, RenderOnce, SharedString, Styled, Transformation,
    Window, img, svg,
};

use crate::color::Color;
use crate::icon_name::IconName;
use crate::spacing::rems_from_px;

/// Where an icon sits next to its label.
#[derive(Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
pub enum IconPosition {
    #[default]
    Start,
    End,
}

/// How big an icon is. Icons are square.
#[derive(Debug, Default, PartialEq, Clone, Copy)]
pub enum IconSize {
    /// 10 px.
    Indicator,
    /// 12 px.
    XSmall,
    /// 14 px.
    Small,
    /// 16 px.
    #[default]
    Medium,
    /// 48 px.
    XLarge,
    Custom(Rems),
}

impl IconSize {
    /// The side in rems, so icons follow the UI size.
    pub fn rems(self) -> Rems {
        match self {
            Self::Indicator => rems_from_px(10_f32),
            Self::XSmall => rems_from_px(12_f32),
            Self::Small => rems_from_px(14_f32),
            Self::Medium => rems_from_px(16_f32),
            Self::XLarge => rems_from_px(48_f32),
            Self::Custom(side) => side,
        }
    }
}

/// What an icon draws.
#[derive(Clone)]
enum Art {
    /// An asset path; the SVG is drawn as a mask in the icon's colour.
    Svg(SharedString),
    /// A picture file, drawn as it is.
    Picture(Arc<Path>),
}

/// An icon: 16 px in the default text colour unless set.
#[derive(Clone, IntoElement)]
pub struct Icon {
    art: Art,
    tint: Color,
    side: AbsoluteLength,
    turn: Transformation,
}

impl Icon {
    pub fn new(icon: IconName) -> Self {
        Self::drawing(Art::Svg(SharedString::from(icon.path())))
    }

    /// A path under `icons/` names an asset SVG; any other path is a picture file on disk.
    pub fn from_path(path: impl Into<SharedString>) -> Self {
        let path: SharedString = path.into();
        if path.starts_with("icons/") {
            return Self::drawing(Art::Svg(path));
        }
        let file: Arc<Path> = PathBuf::from(path.as_ref()).into();
        Self::drawing(Art::Picture(file))
    }

    fn drawing(art: Art) -> Self {
        Icon {
            art,
            tint: Color::Default,
            side: AbsoluteLength::from(IconSize::Medium.rems()),
            turn: Transformation::default(),
        }
    }

    pub fn color(self, color: Color) -> Self {
        Icon {
            tint: color,
            ..self
        }
    }

    pub fn size(self, size: IconSize) -> Self {
        self.length(size.rems().into())
    }

    /// A side of any length; the key hint draws its keys as tall as its text.
    pub(crate) fn length(self, side: AbsoluteLength) -> Self {
        Icon { side, ..self }
    }

    /// Rotates or scales an asset SVG (the spinner of a loading button).
    pub(crate) fn transform(self, turn: Transformation) -> Self {
        Icon { turn, ..self }
    }
}

impl From<IconName> for Icon {
    fn from(icon: IconName) -> Self {
        Self::new(icon)
    }
}

/// The size and colour every kind of icon gets. `flex_none` keeps an icon from shrinking in a
/// tight row.
fn fit<E: Styled>(el: E, side: AbsoluteLength, tint: Hsla) -> E {
    el.flex_none().size(side).text_color(tint)
}

impl RenderOnce for Icon {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let tint = self.tint.color(cx);
        match self.art {
            Art::Svg(path) => fit(svg().path(path), self.side, tint)
                .with_transformation(self.turn)
                .into_any_element(),
            Art::Picture(file) => fit(img(file), self.side, tint).into_any_element(),
        }
    }
}
