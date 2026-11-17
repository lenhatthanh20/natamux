//! The UI kit of the desktop app. The app uses it under the name `ui`. The MIT move
//! (docs/plan-mit-license.md, S8) puts Natamux's own widgets here; only the scrollbars, the menu
//! actions and the menu of Zed's editor still come from Zed's `ui` and `menu` crates.

mod button;
mod button_like;
mod checkbox;
mod chip;
mod color;
mod context_menu;
mod control_chars;
mod disclosure;
mod icon;
mod icon_button;
mod icon_name;
mod indicator;
mod label;
mod list;
mod list_item;
pub mod prelude;
mod shortcut_hint;
mod spacing;
mod switch;
mod tab;
mod toggle_group;
mod tooltip;
mod traits;
pub mod utils;
mod with_rem_size;

pub use button::{Button, KeybindingPosition};
pub use button_like::{
    ButtonCommon, ButtonLike, ButtonSize, ButtonStyle, SelectableButton, TintColor,
};
pub use checkbox::Checkbox;
pub use chip::Chip;
pub use color::Color;
pub use context_menu::{ContextMenu, ContextMenuEntry, ContextMenuItem};
pub use disclosure::Disclosure;
pub use icon::{Icon, IconPosition, IconSize};
pub use icon_button::{IconButton, IconButtonShape};
pub use icon_name::IconName;
pub use indicator::Indicator;
pub use label::{Headline, HeadlineSize, Label, LabelCommon, LabelSize, LineHeightStyle, TextSize};
pub use list_item::{ListItem, ListItemSpacing};
pub use shortcut_hint::KeyBinding;
pub use spacing::{DynamicSpacing, h_flex, rems_from_px, v_flex};
pub use switch::Switch;
pub use tab::{Tab, TabPosition};
pub use toggle_group::{
    ToggleButton, ToggleButtonCell, ToggleButtonGroup, ToggleButtonGroupStyle, ToggleButtonSimple,
    ToggleButtonWithIcon,
};
pub use tooltip::{Tooltip, tooltip_container};
pub use traits::{
    Clickable, Disableable, FixedWidth, StyledExt, ToggleState, Toggleable, VisibleOnHover,
};
pub use with_rem_size::WithRemSize;

/// Zed's scrollbars, until an own one takes their place (after S7).
pub use zed_ui::{ScrollAxes, ScrollableHandle, Scrollbars, WithScrollbar, scrollbars};

/// Zed's menu, for the menu Zed's editor opens itself (`Editor::set_custom_context_menu`), until
/// the own editor takes its place.
pub mod editor_menu {
    pub use zed_ui::{ContextMenu, ContextMenuEntry};
}

/// The actions of an open menu, Zed's types until S8.4 (plan 6.7.1).
pub mod menu {
    pub use zed_menu::*;
}
