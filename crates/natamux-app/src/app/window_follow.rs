//! What Root follows of the dashboard: its new toasts, the refused token, and the parts that
//! the views drawn with Root read. A change of the feed notifies no view by itself (`tell`), so
//! a change that no view on screen shows draws no frame.

use gpui::Entity;
use ui::prelude::*;

use super::Root;
use crate::app::dashboard::Dashboard;
use crate::app::dashboard_data::{Generations, Part, Seen};
use crate::components::dialog::DialogLayer;
use crate::components::menu::MenuLayer;
use crate::components::toast_rules::Toast;
use crate::components::toasts::push_toast;
use crate::palette::palette_layer::PaletteLayer;

/// The parts that every view drawn with Root follows by itself when it reads them: the open
/// workspace its own task's terminals, lines and preparing step, the Agents grid the terminals
/// and the lines. The usage shows only in cached views. Root draws for every other part.
const QUIET_PARTS: &[Part] = &[
    Part::Activity,
    Part::Usage,
    Part::Terminals,
    Part::Preparing,
];

/// What Root took from the dashboard so far.
#[derive(Debug, Default)]
pub(super) struct Followed {
    /// The id of the last dashboard toast shown, so each one shows once.
    toasts_shown: u64,
    refused: bool,
    parts: Seen,
}

impl Followed {
    /// The toasts not shown yet, and whether Root draws again. `layered`: a layer shows, which
    /// draws with Root and may read any part.
    fn take(
        &mut self,
        toasts: &[Toast],
        refused: bool,
        gens: &Generations,
        layered: bool,
    ) -> (Vec<Toast>, bool) {
        let fresh: Vec<Toast> = toasts
            .iter()
            .filter(|toast| toast.id > self.toasts_shown)
            .cloned()
            .collect();
        if let Some(last) = fresh.last() {
            self.toasts_shown = last.id;
        }
        let refused_changed = std::mem::replace(&mut self.refused, refused) != refused;
        let quiet: &[Part] = if layered { &[] } else { QUIET_PARTS };
        let parts_changed = self.parts.take(gens).any_but(quiet);
        let draw = !fresh.is_empty() || refused_changed || parts_changed;
        (fresh, draw)
    }
}

impl Root {
    /// The dashboard keeps its own toast queue; each new one shows once in the window's stack.
    pub(super) fn follow_dashboard(
        &mut self,
        dashboard: Entity<Dashboard>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let layered = self.layered(cx);
        let (fresh, draw) = {
            let dashboard = dashboard.read(cx);
            let data = &dashboard.data;
            let followed = &mut self.followed;
            followed.take(&dashboard.toasts.toasts, data.refused, &data.gens, layered)
        };
        for shown in fresh {
            push_toast(&shown.text, shown.tone, shown.detail, Vec::new(), cx);
        }
        self.after_refused(window, cx);
        if draw {
            cx.notify();
        }
    }

    /// A dialog, a menu, the palette, Settings, Sign in or a tool shows. They draw with Root
    /// and follow no part of their own.
    fn layered(&self, cx: &App) -> bool {
        self.sign_in.is_some()
            || !matches!(self.tool, super::Tool::None)
            || self.dashboard.read(cx).settings.is_some()
            || DialogLayer::is_open(cx)
            || MenuLayer::is_open(cx)
            || PaletteLayer::is_open(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::toast_rules::{ToastTone, Toasts};

    #[test]
    fn draws_only_for_a_new_toast_or_a_change_of_the_refused_token() {
        let mut followed = Followed::default();
        let mut toasts = Toasts::default();
        let gens = Generations::default();
        // Nothing followed yet: the first look draws.
        assert!(followed.take(&toasts.toasts, false, &gens, false).1);
        assert!(!followed.take(&toasts.toasts, false, &gens, false).1);
        toasts.push("Saved.", ToastTone::Info, None, Vec::new());
        let (fresh, draw) = followed.take(&toasts.toasts, false, &gens, false);
        assert_eq!(fresh.len(), 1);
        assert!(draw);
        // The same toast is not news twice.
        assert!(!followed.take(&toasts.toasts, false, &gens, false).1);
        assert!(followed.take(&toasts.toasts, true, &gens, false).1);
        assert!(!followed.take(&toasts.toasts, true, &gens, false).1);
    }

    #[test]
    fn a_part_the_views_follow_themselves_draws_no_frame_unless_a_layer_shows() {
        let mut followed = Followed::default();
        let mut gens = Generations::default();
        followed.take(&[], false, &gens, false);
        for part in QUIET_PARTS {
            gens.bump(*part);
            assert!(!followed.take(&[], false, &gens, false).1, "{part:?}");
            // A dialog may show any part, and has no observer of its own.
            gens.bump(*part);
            assert!(followed.take(&[], false, &gens, true).1, "{part:?}");
        }
        // Root and the views drawn with it read the rest.
        for part in [Part::Tasks, Part::View, Part::Widths, Part::DiffStats] {
            gens.bump(part);
            assert!(followed.take(&[], false, &gens, false).1, "{part:?}");
        }
        // One batch of the feed with both: the loud part draws.
        gens.bump(Part::Usage);
        gens.bump(Part::Jobs);
        assert!(followed.take(&[], false, &gens, false).1);
    }
}
