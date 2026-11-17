//! A segmented control: one row of buttons, one of them selected.

use gpui::{
    AnyView, App, ClickEvent, DefiniteLength, IntoElement, ParentElement, RenderOnce, SharedString,
    Styled, Window, div, prelude::FluentBuilder, relative,
};
use theme::ActiveTheme;

use crate::button_like::{ButtonCommon, ButtonLike, ButtonStyle, SelectableButton, TintColor};
use crate::color::Color;
use crate::icon::{Icon, IconSize};
use crate::icon_name::IconName;
use crate::label::{Label, LabelCommon, LabelSize};
use crate::spacing::{h_flex, v_flex};
use crate::traits::{Clickable, FixedWidth, Toggleable};

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;
type TooltipBuilder = Box<dyn Fn(&mut Window, &mut App) -> AnyView>;

/// What one cell of a group shows and does.
pub struct ToggleButtonCell {
    label: SharedString,
    icon: Option<IconName>,
    on_click: ClickHandler,
    selected: bool,
    tooltip: Option<TooltipBuilder>,
}

/// A kind of cell a group takes.
pub trait ToggleButton: 'static {
    fn into_cell(self) -> ToggleButtonCell;
}

/// A cell with a label.
pub struct ToggleButtonSimple(ToggleButtonCell);

impl ToggleButtonSimple {
    pub fn new(
        label: impl Into<SharedString>,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self(ToggleButtonCell {
            label: label.into(),
            icon: None,
            on_click: Box::new(on_click),
            selected: false,
            tooltip: None,
        })
    }

    /// Selected as well as the group's selected index.
    pub fn selected(mut self, selected: bool) -> Self {
        self.0.selected = selected;
        self
    }

    pub fn tooltip(mut self, tooltip: impl Fn(&mut Window, &mut App) -> AnyView + 'static) -> Self {
        self.0.tooltip = Some(Box::new(tooltip));
        self
    }
}

impl ToggleButton for ToggleButtonSimple {
    fn into_cell(self) -> ToggleButtonCell {
        self.0
    }
}

/// A cell with a small icon before its label.
pub struct ToggleButtonWithIcon(ToggleButtonCell);

impl ToggleButtonWithIcon {
    pub fn new(
        label: impl Into<SharedString>,
        icon: IconName,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self(ToggleButtonCell {
            label: label.into(),
            icon: Some(icon),
            on_click: Box::new(on_click),
            selected: false,
            tooltip: None,
        })
    }

    /// Selected as well as the group's selected index.
    pub fn selected(mut self, selected: bool) -> Self {
        self.0.selected = selected;
        self
    }

    pub fn tooltip(mut self, tooltip: impl Fn(&mut Window, &mut App) -> AnyView + 'static) -> Self {
        self.0.tooltip = Some(Box::new(tooltip));
        self
    }
}

impl ToggleButton for ToggleButtonWithIcon {
    fn into_cell(self) -> ToggleButtonCell {
        self.0
    }
}

/// The frame of a group.
#[derive(Clone, Copy, PartialEq)]
pub enum ToggleButtonGroupStyle {
    /// No frame; 1 px gaps between the cells.
    Transparent,
    /// A frame and lines between the cells, and filled cells.
    Filled,
    /// A frame and lines between the cells.
    Outlined,
}

/// `COLS` cells in a row. Each cell is as wide as the group over `COLS`, unless `auto_width`.
#[derive(IntoElement)]
pub struct ToggleButtonGroup<T: ToggleButton, const COLS: usize = 3> {
    name: SharedString,
    cells: [T; COLS],
    style: ToggleButtonGroupStyle,
    label_size: LabelSize,
    width: Option<DefiniteLength>,
    auto_width: bool,
    selected_index: usize,
    tab_index: Option<isize>,
}

impl<T: ToggleButton, const COLS: usize> ToggleButtonGroup<T, COLS> {
    pub fn single_row(name: impl Into<SharedString>, cells: [T; COLS]) -> Self {
        Self {
            name: name.into(),
            cells,
            style: ToggleButtonGroupStyle::Transparent,
            label_size: LabelSize::Small,
            width: None,
            auto_width: false,
            selected_index: 0,
            tab_index: None,
        }
    }

    pub fn style(mut self, style: ToggleButtonGroupStyle) -> Self {
        self.style = style;
        self
    }

    pub fn selected_index(mut self, index: usize) -> Self {
        self.selected_index = index;
        self
    }

    /// Each cell as wide as its content.
    pub fn auto_width(mut self) -> Self {
        self.auto_width = true;
        self
    }

    pub fn label_size(mut self, label_size: LabelSize) -> Self {
        self.label_size = label_size;
        self
    }

    /// The cells take the tab stops from `tab_index` on, which moves past them.
    pub fn tab_index(mut self, tab_index: &mut isize) -> Self {
        self.tab_index = Some(*tab_index);
        *tab_index += COLS as isize;
        self
    }
}

impl<T: ToggleButton, const COLS: usize> FixedWidth for ToggleButtonGroup<T, COLS> {
    fn width(mut self, width: impl Into<DefiniteLength>) -> Self {
        self.width = Some(width.into());
        self
    }

    fn full_width(mut self) -> Self {
        self.width = Some(relative(1.));
        self
    }
}

impl<T: ToggleButton, const COLS: usize> RenderOnce for ToggleButtonGroup<T, COLS> {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let line = cx.theme().colors().border.opacity(0.6);
        let framed = self.style != ToggleButtonGroupStyle::Transparent;
        let auto_width = self.auto_width;
        let label_size = self.label_size;
        let selected_index = self.selected_index;
        let tab_index = self.tab_index;
        let filled = self.style == ToggleButtonGroupStyle::Filled;
        let name = self.name;

        let cells = self.cells.into_iter().enumerate().map(|(index, cell)| {
            let ToggleButtonCell {
                label,
                icon,
                on_click,
                selected,
                tooltip,
            } = cell.into_cell();
            let on = selected || index == selected_index;
            let last = index + 1 == COLS;
            let content = h_flex()
                .w_full()
                .px_2()
                .gap_1p5()
                .justify_center()
                .flex_none()
                .when_some(icon, |row, icon| {
                    let color = if on { Color::Accent } else { Color::Muted };
                    row.py_2()
                        .child(Icon::new(icon).size(IconSize::XSmall).color(color))
                })
                .child(
                    Label::new(label)
                        .size(label_size)
                        .when(on, |label| label.color(Color::Accent)),
                );
            let button = ButtonLike::new((name.clone(), index))
                .when(!auto_width, |button| button.full_width())
                .round_sides(index == 0, last)
                .when_some(tab_index, |button, first| {
                    button.tab_index(first + index as isize)
                })
                .when(on, |button| {
                    button
                        .toggle_state(true)
                        .selected_style(ButtonStyle::Tinted(TintColor::Accent))
                })
                .when(filled, |button| button.style(ButtonStyle::Filled))
                .child(content)
                .when_some(tooltip, |button, tooltip| {
                    button.tooltip(move |window, cx| tooltip(window, cx))
                })
                .on_click(on_click);
            div()
                .when(framed && !last, |cell| cell.border_r_1().border_color(line))
                .when(!auto_width, |cell| cell.w(relative(1. / COLS as f32)))
                .overflow_hidden()
                .child(button)
        });

        v_flex()
            .map(|group| match self.width {
                Some(width) => group.w(width),
                None if auto_width => group,
                None => group.w_full(),
            })
            .rounded_md()
            .overflow_hidden()
            .map(|group| {
                if framed {
                    group.border_1().border_color(line)
                } else {
                    group.gap_px()
                }
            })
            .child(h_flex().when(!framed, |row| row.gap_px()).children(cells))
    }
}
