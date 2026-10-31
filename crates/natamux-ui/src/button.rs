//! A button with a label, optional icons before and after it, and an optional key hint.

use std::time::Duration;

use gpui::{
    Animation, AnimationExt, AnyView, App, ClickEvent, CursorStyle, DefiniteLength, ElementId,
    FocusHandle, IntoElement, ParentElement, RenderOnce, SharedString, Styled, Transformation,
    Window, percentage, prelude::FluentBuilder,
};
use zed_ui::{
    ButtonSize, ButtonStyle, Clickable, Color, Disableable, FixedWidth, IconName, IconSize,
    LabelSize, Toggleable,
};

use crate::button_like::{ButtonCommon, ButtonLike, SelectableButton};
use crate::icon::Icon;
use crate::label::{Label, LabelCommon};
use crate::shortcut_hint::KeyBinding;
use crate::spacing::{DynamicSpacing, h_flex};

/// Which side of the label the key hint is on.
#[derive(Default, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy)]
pub enum KeybindingPosition {
    Start,
    #[default]
    End,
}

#[derive(IntoElement)]
pub struct Button {
    base: ButtonLike,
    label: SharedString,
    label_color: Option<Color>,
    label_size: Option<LabelSize>,
    selected_label: Option<SharedString>,
    selected_label_color: Option<Color>,
    start_icon: Option<Icon>,
    end_icon: Option<Icon>,
    key_binding: Option<KeyBinding>,
    key_binding_position: KeybindingPosition,
    alpha: Option<f32>,
    truncate: bool,
    loading: bool,
}

impl Button {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            base: ButtonLike::new(id),
            label: label.into(),
            label_color: None,
            label_size: None,
            selected_label: None,
            selected_label_color: None,
            start_icon: None,
            end_icon: None,
            key_binding: None,
            key_binding_position: KeybindingPosition::default(),
            alpha: None,
            truncate: false,
            loading: false,
        }
    }

    /// The label colour while the button is enabled and not selected.
    pub fn color(mut self, label_color: impl Into<Option<Color>>) -> Self {
        self.label_color = label_color.into();
        self
    }

    pub fn label_size(mut self, label_size: impl Into<Option<LabelSize>>) -> Self {
        self.label_size = label_size.into();
        self
    }

    pub fn selected_label<L: Into<SharedString>>(mut self, label: impl Into<Option<L>>) -> Self {
        self.selected_label = label.into().map(Into::into);
        self
    }

    pub fn selected_label_color(mut self, color: impl Into<Option<Color>>) -> Self {
        self.selected_label_color = color.into();
        self
    }

    pub fn start_icon(mut self, icon: impl Into<Option<Icon>>) -> Self {
        self.start_icon = icon.into();
        self
    }

    pub fn end_icon(mut self, icon: impl Into<Option<Icon>>) -> Self {
        self.end_icon = icon.into();
        self
    }

    pub fn key_binding(mut self, key_binding: impl Into<Option<KeyBinding>>) -> Self {
        self.key_binding = key_binding.into();
        self
    }

    pub fn key_binding_position(mut self, position: KeybindingPosition) -> Self {
        self.key_binding_position = position;
        self
    }

    pub fn alpha(mut self, alpha: f32) -> Self {
        self.alpha = Some(alpha);
        self
    }

    /// Lets the label shrink and end in an ellipsis.
    pub fn truncate(mut self, truncate: bool) -> Self {
        self.truncate = truncate;
        self
    }

    /// A turning spinner in place of the start icon.
    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }

    pub fn aria_label(mut self, label: impl Into<SharedString>) -> Self {
        self.base = self.base.aria_label(label);
        self
    }
}

impl Toggleable for Button {
    fn toggle_state(mut self, selected: bool) -> Self {
        self.base = self.base.toggle_state(selected);
        self
    }
}

impl SelectableButton for Button {
    fn selected_style(mut self, style: ButtonStyle) -> Self {
        self.base = self.base.selected_style(style);
        self
    }
}

impl Disableable for Button {
    fn disabled(mut self, disabled: bool) -> Self {
        self.base = self.base.disabled(disabled);
        self
    }
}

impl Clickable for Button {
    fn on_click(mut self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.base = self.base.on_click(handler);
        self
    }

    fn cursor_style(mut self, cursor_style: CursorStyle) -> Self {
        self.base = self.base.cursor_style(cursor_style);
        self
    }
}

impl FixedWidth for Button {
    fn width(mut self, width: impl Into<DefiniteLength>) -> Self {
        self.base = self.base.width(width);
        self
    }

    fn full_width(mut self) -> Self {
        self.base = self.base.full_width();
        self
    }
}

impl ButtonCommon for Button {
    fn id(&self) -> &ElementId {
        self.base.id()
    }

    fn style(mut self, style: ButtonStyle) -> Self {
        self.base = self.base.style(style);
        self
    }

    fn size(mut self, size: ButtonSize) -> Self {
        self.base = self.base.size(size);
        self
    }

    fn tooltip(mut self, tooltip: impl Fn(&mut Window, &mut App) -> AnyView + 'static) -> Self {
        self.base = self.base.tooltip(tooltip);
        self
    }

    fn tab_index(mut self, tab_index: impl Into<isize>) -> Self {
        self.base = self.base.tab_index(tab_index);
        self
    }

    fn track_focus(mut self, focus_handle: &FocusHandle) -> Self {
        self.base = self.base.track_focus(focus_handle);
        self
    }
}

impl RenderOnce for Button {
    #[allow(refining_impl_trait)]
    fn render(mut self, _: &mut Window, cx: &mut App) -> ButtonLike {
        let disabled = self.base.disabled;
        let selected = self.base.selected;
        let label = self
            .selected_label
            .filter(|_| selected)
            .unwrap_or(self.label);
        if self.base.aria_label.is_none() {
            self.base.aria_label = Some(label.clone());
        }
        if self.base.aria_keyshortcuts.is_none() {
            self.base.aria_keyshortcuts = self.key_binding.as_ref().map(KeyBinding::text);
        }
        let label_color = if disabled {
            Color::Disabled
        } else if selected {
            self.selected_label_color.unwrap_or(Color::Selected)
        } else {
            self.label_color.unwrap_or_default()
        };
        let dim = |icon: Icon| {
            if disabled {
                icon.color(Color::Disabled)
            } else {
                icon
            }
        };
        let truncate = self.truncate;
        let spinner = self.loading.then(|| {
            Icon::new(IconName::LoadCircle)
                .size(IconSize::Small)
                .color(Color::Muted)
                .with_animation(
                    (self.base.id().clone(), "loading"),
                    Animation::new(Duration::from_secs(2)).repeat_synced(),
                    |icon, turn| icon.transform(Transformation::rotate(percentage(turn))),
                )
        });

        let text = h_flex()
            .when(truncate, |this| this.min_w_0().overflow_hidden())
            .when(
                self.key_binding_position == KeybindingPosition::Start,
                |this| this.flex_row_reverse(),
            )
            .gap(DynamicSpacing::Base06.rems(cx))
            .justify_between()
            .child(
                Label::new(label)
                    .color(label_color)
                    .size(self.label_size.unwrap_or_default())
                    .when_some(self.alpha, |this, alpha| this.alpha(alpha))
                    .when(truncate, |this| this.truncate()),
            )
            .children(self.key_binding);

        self.base.child(
            h_flex()
                .when(truncate, |this| this.min_w_0().overflow_hidden())
                .gap(DynamicSpacing::Base04.rems(cx))
                .map(|this| match spinner {
                    Some(spinner) => this.child(spinner),
                    None => this.when_some(self.start_icon, |this, icon| this.child(dim(icon))),
                })
                .child(text)
                .when_some(self.end_icon, |this, icon| this.child(dim(icon))),
        )
    }
}
