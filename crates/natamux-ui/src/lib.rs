//! The UI kit of the desktop app. The app uses it under the name `ui`. The MIT move
//! (docs/plan-mit-license.md, S8) puts Natamux's own widgets here one by one; the rest still
//! comes from Zed's `ui` crate, so the app's `ui::` paths stay as they are. A name this crate
//! defines hides Zed's item of the same name.

mod button;
mod button_like;
mod context_menu;
mod control_chars;
mod icon;
mod icon_button;
mod label;
mod list;
pub mod prelude;
mod shortcut_hint;
mod spacing;
mod tooltip;
mod with_rem_size;

pub use zed_ui::*;

pub use button::{Button, KeybindingPosition};
pub use button_like::{ButtonCommon, ButtonLike, SelectableButton};
pub use context_menu::{ContextMenu, ContextMenuEntry, ContextMenuItem};
pub use icon::Icon;
pub use icon_button::{IconButton, IconButtonShape};
pub use label::{Headline, HeadlineSize, Label, LabelCommon, LineHeightStyle};
pub use shortcut_hint::KeyBinding;
pub use spacing::{DynamicSpacing, h_flex, rems_from_px, v_flex};
pub use tooltip::{Tooltip, tooltip_container};

/// The actions of an open menu, Zed's types until S8.4 (plan 6.7.1).
pub mod menu {
    pub use zed_menu::*;
}
