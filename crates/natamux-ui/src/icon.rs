//! An icon: one of the app's SVG assets tinted with a theme colour, or an image file.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::{
    AbsoluteLength, App, IntoElement, RenderOnce, SharedString, Styled, Transformation, Window,
    img, svg,
};
use zed_ui::{Color, IconName, IconSize};

#[derive(Clone)]
enum Source {
    /// An SVG of the app's assets, tinted.
    Asset(SharedString),
    /// An image file on disk.
    File(Arc<Path>),
}

/// An icon, 16 px and in the default text colour unless set.
#[derive(Clone, IntoElement)]
pub struct Icon {
    source: Source,
    color: Color,
    size: AbsoluteLength,
    transformation: Transformation,
}

impl Icon {
    pub fn new(icon: IconName) -> Self {
        Self::of(Source::Asset(icon.path().into()))
    }

    /// An asset path (`icons/...`) is a tinted SVG; any other path is an image file.
    pub fn from_path(path: impl Into<SharedString>) -> Self {
        let path = path.into();
        if path.starts_with("icons/") {
            Self::of(Source::Asset(path))
        } else {
            Self::of(Source::File(Arc::from(PathBuf::from(path.as_ref()))))
        }
    }

    fn of(source: Source) -> Self {
        Self {
            source,
            color: Color::default(),
            size: IconSize::default().rems().into(),
            transformation: Transformation::default(),
        }
    }

    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    pub fn size(mut self, size: IconSize) -> Self {
        self.size = size.rems().into();
        self
    }

    /// Any length, for the kit's own widgets (the key hint draws its keys at the text size).
    pub(crate) fn length(mut self, size: AbsoluteLength) -> Self {
        self.size = size;
        self
    }

    /// Turns or scales an SVG icon (the spinner of a loading button).
    pub(crate) fn transform(mut self, transformation: Transformation) -> Self {
        self.transformation = transformation;
        self
    }
}

impl From<IconName> for Icon {
    fn from(icon: IconName) -> Self {
        Icon::new(icon)
    }
}

impl RenderOnce for Icon {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let color = self.color.color(cx);
        match self.source {
            Source::Asset(path) => svg()
                .with_transformation(self.transformation)
                .size(self.size)
                .flex_none()
                .path(path)
                .text_color(color)
                .into_any_element(),
            Source::File(path) => img(path)
                .size(self.size)
                .flex_none()
                .text_color(color)
                .into_any_element(),
        }
    }
}
