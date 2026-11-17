//! A button with a label, optional icons before and after it, and an optional key hint.

use std::time::Duration;

use gpui::{
    Animation, AnimationExt, AnyElement, App, Div, ElementId, IntoElement, ParentElement,
    RenderOnce, SharedString, Styled, Transformation, Window, percentage,
};

use crate::button_like::ButtonLike;
use crate::button_setters::forward_button_setters;
use crate::color::Color;
use crate::icon::{Icon, IconSize};
use crate::icon_name::IconName;
use crate::label::{Label, LabelCommon, LabelSize};
use crate::shortcut_hint::KeyBinding;
use crate::spacing::{DynamicSpacing, h_flex};

/// Which side of the label the key hint is on.
#[derive(Clone, Copy, Debug, Default, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum KeybindingPosition {
    Start,
    #[default]
    End,
}

#[derive(IntoElement)]
pub struct Button {
    base: ButtonLike,
    text: SharedString,
    /// The text colour while the button is enabled and not selected.
    text_color: Option<Color>,
    text_size: Option<LabelSize>,
    before: Option<Icon>,
    after: Option<Icon>,
    hint: Option<KeyBinding>,
    hint_first: bool,
    shrinks: bool,
    spinning: bool,
}

forward_button_setters!(Button, base);

impl Button {
    pub fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            base: ButtonLike::new(id),
            text: label.into(),
            text_color: None,
            text_size: None,
            before: None,
            after: None,
            hint: None,
            hint_first: false,
            shrinks: false,
            spinning: false,
        }
    }

    pub fn color(self, label_color: impl Into<Option<Color>>) -> Self {
        Self {
            text_color: label_color.into(),
            ..self
        }
    }

    pub fn label_size(self, label_size: impl Into<Option<LabelSize>>) -> Self {
        Self {
            text_size: label_size.into(),
            ..self
        }
    }

    pub fn start_icon(self, icon: impl Into<Option<Icon>>) -> Self {
        Self {
            before: icon.into(),
            ..self
        }
    }

    pub fn end_icon(self, icon: impl Into<Option<Icon>>) -> Self {
        Self {
            after: icon.into(),
            ..self
        }
    }

    pub fn key_binding(self, key_binding: impl Into<Option<KeyBinding>>) -> Self {
        Self {
            hint: key_binding.into(),
            ..self
        }
    }

    pub fn key_binding_position(self, position: KeybindingPosition) -> Self {
        Self {
            hint_first: position == KeybindingPosition::Start,
            ..self
        }
    }

    /// Lets the label shrink and end in an ellipsis.
    pub fn truncate(self, truncate: bool) -> Self {
        Self {
            shrinks: truncate,
            ..self
        }
    }

    /// A turning spinner in place of the start icon.
    pub fn loading(self, loading: bool) -> Self {
        Self {
            spinning: loading,
            ..self
        }
    }
}

/// The text colour: grey while off, the accent while selected, else the colour the caller set.
fn text_colour(disabled: bool, selected: bool, set: Option<Color>) -> Color {
    match (disabled, selected) {
        (true, _) => Color::Disabled,
        (false, true) => Color::Selected,
        (false, false) => set.unwrap_or_default(),
    }
}

/// A row that may shrink below its content, so a label inside it can end in an ellipsis.
fn may_shrink(row: Div, shrinks: bool) -> Div {
    if shrinks {
        row.min_w_0().overflow_hidden()
    } else {
        row
    }
}

impl RenderOnce for Button {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self {
            mut base,
            text,
            text_color,
            text_size,
            before,
            after,
            hint,
            hint_first,
            shrinks,
            spinning,
        } = self;
        let off = base.setup.disabled;
        let colour = text_colour(off, base.setup.is_on(), text_color);
        // Screen readers say the label and the keys unless the caller named them.
        if base.setup.spoken.is_none() {
            base.setup.spoken = Some(text.clone());
        }
        if base.setup.shortcut.is_none() {
            base.setup.shortcut = hint.as_ref().map(KeyBinding::text);
        }

        let grey_if_off = |icon: Icon| if off { icon.color(Color::Disabled) } else { icon };
        let lead: Option<AnyElement> = if spinning {
            let spinner = Icon::new(IconName::LoadCircle)
                .size(IconSize::Small)
                .color(Color::Muted)
                .with_animation(
                    (base.setup.id.clone(), "loading"),
                    Animation::new(Duration::from_secs(2)).repeat_synced(),
                    |icon, turn| icon.transform(Transformation::rotate(percentage(turn))),
                );
            Some(spinner.into_any_element())
        } else {
            before.map(|icon| grey_if_off(icon).into_any_element())
        };

        let mut caption = Label::new(text)
            .color(colour)
            .size(text_size.unwrap_or_default());
        if shrinks {
            caption = caption.truncate();
        }
        let mut words = h_flex()
            .gap(DynamicSpacing::Base06.rems(cx))
            .justify_between()
            .child(caption)
            .children(hint);
        if hint_first {
            words = words.flex_row_reverse();
        }
        let row = h_flex()
            .gap(DynamicSpacing::Base04.rems(cx))
            .children(lead)
            .child(may_shrink(words, shrinks))
            .children(after.map(grey_if_off));
        base.child(may_shrink(row, shrinks))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::button_like::{ButtonCommon, ButtonSize, ButtonStyle};
    use crate::traits::{Disableable, FixedWidth, Toggleable};

    #[test]
    fn the_text_colour_follows_off_then_selected_then_the_set_colour() {
        assert_eq!(text_colour(true, true, Some(Color::Error)), Color::Disabled);
        assert_eq!(text_colour(false, true, Some(Color::Error)), Color::Selected);
        assert_eq!(text_colour(false, false, Some(Color::Error)), Color::Error);
        assert_eq!(text_colour(false, false, None), Color::Default);
    }

    #[test]
    fn a_button_hands_its_box_setters_to_the_box() {
        let button = Button::new("b", "Go")
            .disabled(true)
            .toggle_state(true)
            .style(ButtonStyle::Outlined)
            .size(ButtonSize::Large)
            .full_width()
            .tab_index(3_isize);
        assert!(button.base.setup.disabled);
        assert!(button.base.setup.is_on());
        assert_eq!(button.id(), &ElementId::from("b"));
    }

    #[test]
    fn the_hint_goes_first_only_at_the_start() {
        let button = Button::new("b", "Go");
        assert!(!button.hint_first);
        let button = button.key_binding_position(KeybindingPosition::Start);
        assert!(button.hint_first);
        let button = button.key_binding_position(KeybindingPosition::End);
        assert!(!button.hint_first);
    }
}
