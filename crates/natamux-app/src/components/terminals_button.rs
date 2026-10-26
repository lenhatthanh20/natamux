//! The **Terminals** button at the end of the toolbar of a screen that covers the terminals in
//! main (Changes, Docs, the open file; T-0183): it shows the terminals again, as Esc does.

use ui::prelude::*;

use super::button_style::{button_icon, key_hint, quiet_button};

/// The caller adds the click, which shows the terminals (`Dashboard::pick_tab(Terminal)`).
pub fn terminals_button(id: impl Into<ElementId>) -> Button {
    quiet_button(id, "Terminals")
        .label_size(LabelSize::Small)
        .start_icon(button_icon("terminal"))
        .key_binding(key_hint("escape", false))
}
