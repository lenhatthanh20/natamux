//! The colours of a theme. The field names are the theme file's keys with `_` for `.`, so a key
//! such as `border.variant` sets `border_variant`, and the names the views read stay the ones
//! they read today.

use std::sync::Arc;

use gpui::Hsla;

/// One colour field: the theme file or the fallback rows fill it.
pub(crate) trait Slot {
    fn get(&self) -> Option<Hsla>;
    /// False when the field needs a colour and `value` is None.
    fn put(&mut self, value: Option<Hsla>) -> bool;
}

impl Slot for Hsla {
    fn get(&self) -> Option<Hsla> {
        Some(*self)
    }

    fn put(&mut self, value: Option<Hsla>) -> bool {
        match value {
            Some(value) => {
                *self = value;
                true
            }
            None => false,
        }
    }
}

impl Slot for Option<Hsla> {
    fn get(&self) -> Option<Hsla> {
        *self
    }

    fn put(&mut self, value: Option<Hsla>) -> bool {
        *self = value;
        true
    }
}

/// A struct of colour fields that the loader fills by name.
pub(crate) trait Fields: Default {
    const NAMES: &'static [&'static str];
    fn slot(&mut self, name: &str) -> Option<&mut dyn Slot>;
}

macro_rules! colour_fields {
    ($(#[$doc:meta])* $name:ident { $($field:ident: $ty:ty,)* }) => {
        $(#[$doc])*
        #[derive(Clone, Debug, Default)]
        pub struct $name {
            $(pub $field: $ty,)*
        }

        impl Fields for $name {
            const NAMES: &'static [&'static str] = &[$(stringify!($field)),*];

            fn slot(&mut self, name: &str) -> Option<&mut dyn Slot> {
                match name {
                    $(stringify!($field) => Some(&mut self.$field as &mut dyn Slot),)*
                    _ => None,
                }
            }
        }

        impl $name {
            /// Every field with its value, in the order of the struct; None for an unset optional
            /// field.
            pub fn fields(&self) -> Vec<(&'static str, Option<Hsla>)> {
                vec![$((stringify!($field), Slot::get(&self.$field)),)*]
            }

            /// Sets the field `name`; false for a name that is not a field.
            pub fn set(&mut self, name: &str, value: Hsla) -> bool {
                self.slot(name).is_some_and(|slot| slot.put(Some(value)))
            }
        }
    };
}

colour_fields! {
    /// The colours of the UI, the editors and the terminal.
    ThemeColors {
        border: Hsla,
        border_variant: Hsla,
        border_focused: Hsla,
        border_selected: Hsla,
        border_transparent: Hsla,
        border_disabled: Hsla,
        elevated_surface_background: Hsla,
        surface_background: Hsla,
        background: Hsla,
        element_background: Hsla,
        element_hover: Hsla,
        element_active: Hsla,
        element_selected: Hsla,
        element_selection_background: Hsla,
        element_disabled: Hsla,
        drop_target_background: Hsla,
        drop_target_border: Hsla,
        ghost_element_background: Hsla,
        ghost_element_hover: Hsla,
        ghost_element_active: Hsla,
        ghost_element_selected: Hsla,
        ghost_element_disabled: Hsla,
        text: Hsla,
        text_muted: Hsla,
        text_placeholder: Hsla,
        text_disabled: Hsla,
        text_accent: Hsla,
        icon: Hsla,
        icon_muted: Hsla,
        icon_disabled: Hsla,
        icon_placeholder: Hsla,
        icon_accent: Hsla,
        debugger_accent: Hsla,
        status_bar_background: Hsla,
        title_bar_background: Hsla,
        title_bar_inactive_background: Hsla,
        toolbar_background: Hsla,
        tab_bar_background: Hsla,
        tab_inactive_background: Hsla,
        tab_active_background: Hsla,
        search_match_background: Hsla,
        search_active_match_background: Hsla,
        panel_background: Hsla,
        panel_focused_border: Hsla,
        panel_indent_guide: Hsla,
        panel_indent_guide_hover: Hsla,
        panel_indent_guide_active: Hsla,
        panel_overlay_background: Hsla,
        panel_overlay_hover: Hsla,
        pane_focused_border: Hsla,
        pane_group_border: Hsla,
        scrollbar_thumb_background: Hsla,
        scrollbar_thumb_hover_background: Hsla,
        scrollbar_thumb_active_background: Hsla,
        scrollbar_thumb_border: Hsla,
        scrollbar_track_background: Hsla,
        scrollbar_track_border: Hsla,
        minimap_thumb_background: Hsla,
        minimap_thumb_hover_background: Hsla,
        minimap_thumb_active_background: Hsla,
        minimap_thumb_border: Hsla,
        vim_normal_background: Hsla,
        vim_insert_background: Hsla,
        vim_replace_background: Hsla,
        vim_visual_background: Hsla,
        vim_visual_line_background: Hsla,
        vim_visual_block_background: Hsla,
        vim_yank_background: Hsla,
        vim_helix_jump_label_foreground: Hsla,
        vim_helix_normal_background: Hsla,
        vim_helix_select_background: Hsla,
        vim_normal_foreground: Hsla,
        vim_insert_foreground: Hsla,
        vim_replace_foreground: Hsla,
        vim_visual_foreground: Hsla,
        vim_visual_line_foreground: Hsla,
        vim_visual_block_foreground: Hsla,
        vim_helix_normal_foreground: Hsla,
        vim_helix_select_foreground: Hsla,
        editor_foreground: Hsla,
        editor_code_lens_foreground: Option<Hsla>,
        editor_background: Hsla,
        editor_gutter_background: Hsla,
        editor_subheader_background: Hsla,
        editor_active_line_background: Hsla,
        editor_highlighted_line_background: Hsla,
        editor_debugger_active_line_background: Hsla,
        editor_line_number: Hsla,
        editor_active_line_number: Hsla,
        editor_hover_line_number: Hsla,
        editor_invisible: Hsla,
        editor_wrap_guide: Hsla,
        editor_active_wrap_guide: Hsla,
        editor_indent_guide: Hsla,
        editor_indent_guide_active: Hsla,
        editor_document_highlight_read_background: Hsla,
        editor_document_highlight_write_background: Hsla,
        editor_document_highlight_bracket_background: Hsla,
        editor_diff_hunk_added_background: Hsla,
        editor_diff_hunk_added_hollow_background: Hsla,
        editor_diff_hunk_added_hollow_border: Hsla,
        editor_diff_hunk_deleted_background: Hsla,
        editor_diff_hunk_deleted_hollow_background: Hsla,
        editor_diff_hunk_deleted_hollow_border: Hsla,
        terminal_background: Hsla,
        terminal_foreground: Hsla,
        terminal_bright_foreground: Hsla,
        terminal_dim_foreground: Hsla,
        terminal_ansi_background: Hsla,
        terminal_ansi_black: Hsla,
        terminal_ansi_bright_black: Hsla,
        terminal_ansi_dim_black: Hsla,
        terminal_ansi_red: Hsla,
        terminal_ansi_bright_red: Hsla,
        terminal_ansi_dim_red: Hsla,
        terminal_ansi_green: Hsla,
        terminal_ansi_bright_green: Hsla,
        terminal_ansi_dim_green: Hsla,
        terminal_ansi_yellow: Hsla,
        terminal_ansi_bright_yellow: Hsla,
        terminal_ansi_dim_yellow: Hsla,
        terminal_ansi_blue: Hsla,
        terminal_ansi_bright_blue: Hsla,
        terminal_ansi_dim_blue: Hsla,
        terminal_ansi_magenta: Hsla,
        terminal_ansi_bright_magenta: Hsla,
        terminal_ansi_dim_magenta: Hsla,
        terminal_ansi_cyan: Hsla,
        terminal_ansi_bright_cyan: Hsla,
        terminal_ansi_dim_cyan: Hsla,
        terminal_ansi_white: Hsla,
        terminal_ansi_bright_white: Hsla,
        terminal_ansi_dim_white: Hsla,
        link_text_hover: Hsla,
        version_control_added: Hsla,
        version_control_deleted: Hsla,
        version_control_modified: Hsla,
        version_control_renamed: Hsla,
        version_control_conflict: Hsla,
        version_control_ignored: Hsla,
        version_control_word_added: Hsla,
        version_control_word_deleted: Hsla,
        version_control_conflict_marker_ours: Hsla,
        version_control_conflict_marker_theirs: Hsla,
    }
}

colour_fields! {
    /// The colours of states: errors, warnings, changes and the like, each with a background and
    /// a border.
    StatusColors {
        conflict: Hsla,
        conflict_background: Hsla,
        conflict_border: Hsla,
        created: Hsla,
        created_background: Hsla,
        created_border: Hsla,
        deleted: Hsla,
        deleted_background: Hsla,
        deleted_border: Hsla,
        error: Hsla,
        error_background: Hsla,
        error_border: Hsla,
        hidden: Hsla,
        hidden_background: Hsla,
        hidden_border: Hsla,
        hint: Hsla,
        hint_background: Hsla,
        hint_border: Hsla,
        ignored: Hsla,
        ignored_background: Hsla,
        ignored_border: Hsla,
        info: Hsla,
        info_background: Hsla,
        info_border: Hsla,
        modified: Hsla,
        modified_background: Hsla,
        modified_border: Hsla,
        predictive: Hsla,
        predictive_background: Hsla,
        predictive_border: Hsla,
        renamed: Hsla,
        renamed_background: Hsla,
        renamed_border: Hsla,
        success: Hsla,
        success_background: Hsla,
        success_border: Hsla,
        unreachable: Hsla,
        unreachable_background: Hsla,
        unreachable_border: Hsla,
        warning: Hsla,
        warning_background: Hsla,
        warning_border: Hsla,
    }
}

/// The caret and selection colours of one person in an editor.
#[derive(Clone, Copy, Debug, Default)]
pub struct PlayerColor {
    pub cursor: Hsla,
    pub background: Hsla,
    pub selection: Hsla,
}

/// The first entry is the local person.
#[derive(Clone, Debug, Default)]
pub struct PlayerColors(pub Vec<PlayerColor>);

impl PlayerColors {
    pub fn local(&self) -> PlayerColor {
        self.0.first().copied().unwrap_or_default()
    }

    /// The local colours in grey, for text the person cannot edit.
    pub fn read_only(&self) -> PlayerColor {
        let local = self.local();
        PlayerColor {
            cursor: local.cursor.grayscale(),
            background: local.background.grayscale(),
            selection: local.selection.grayscale(),
        }
    }
}

/// Colours that repeat, such as for nested brackets.
#[derive(Clone, Debug, Default)]
pub struct AccentColors(pub Arc<[Hsla]>);

impl AccentColors {
    pub fn color_for_index(&self, index: u32) -> Hsla {
        let len = self.0.len().max(1);
        self.0.get(index as usize % len).copied().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use gpui::{black, white};

    use super::*;

    #[test]
    fn sets_a_field_by_its_name_only() {
        let mut colors = ThemeColors::default();
        assert!(colors.set("border_variant", white()));
        assert_eq!(colors.border_variant, white());
        assert!(!colors.set("border.variant", white()));
        assert!(colors.set("editor_code_lens_foreground", black()));
        assert_eq!(colors.editor_code_lens_foreground, Some(black()));
    }

    #[test]
    fn a_needed_colour_takes_no_none_but_an_optional_one_does() {
        let mut colors = ThemeColors::default();
        assert!(!colors.slot("text").is_some_and(|slot| slot.put(None)));
        assert!(
            colors
                .slot("editor_code_lens_foreground")
                .is_some_and(|slot| slot.put(None))
        );
    }

    #[test]
    fn lists_every_field_in_order() {
        let colors = ThemeColors::default().fields();
        assert_eq!(colors.len(), 144);
        assert_eq!(colors[0].0, "border");
        assert_eq!(ThemeColors::NAMES.len(), 144);
        assert_eq!(StatusColors::NAMES.len(), 42);
        assert_eq!(StatusColors::default().fields()[0].0, "conflict");
    }

    #[test]
    fn accents_repeat_and_players_start_with_the_local_one() {
        let accents = AccentColors(Arc::from([black(), white()]));
        assert_eq!(accents.color_for_index(3), white());
        assert_eq!(AccentColors::default().color_for_index(2), Hsla::default());
        let local = PlayerColor {
            cursor: white(),
            ..PlayerColor::default()
        };
        let players = PlayerColors(vec![local, PlayerColor::default()]);
        assert_eq!(players.local().cursor, white());
        assert_eq!(players.read_only().cursor, white().grayscale());
    }
}
