//! Root follows the dashboard for two things only: its new toasts and the refused token. The
//! window draws again for any dashboard notify that a view follows, so Root itself draws again
//! only when one of those two changed (T-0223, P2).

use gpui::Entity;
use ui::prelude::*;

use super::Root;
use crate::app::dashboard::Dashboard;
use crate::components::toast_rules::Toast;
use crate::components::toasts::push_toast;

/// What Root took from the dashboard so far.
#[derive(Debug, Default)]
pub(super) struct Followed {
    /// The id of the last dashboard toast shown, so each one shows once.
    toasts_shown: u64,
    refused: bool,
}

impl Followed {
    /// The toasts not shown yet, and whether Root draws again.
    fn take(&mut self, toasts: &[Toast], refused: bool) -> (Vec<Toast>, bool) {
        let fresh: Vec<Toast> = toasts
            .iter()
            .filter(|toast| toast.id > self.toasts_shown)
            .cloned()
            .collect();
        if let Some(last) = fresh.last() {
            self.toasts_shown = last.id;
        }
        let refused_changed = std::mem::replace(&mut self.refused, refused) != refused;
        let draw = !fresh.is_empty() || refused_changed;
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
        let (fresh, draw) = {
            let dashboard = dashboard.read(cx);
            let toasts = &dashboard.toasts.toasts;
            self.followed.take(toasts, dashboard.data.refused)
        };
        for shown in fresh {
            push_toast(&shown.text, shown.tone, shown.detail, Vec::new(), cx);
        }
        self.after_refused(window, cx);
        if draw {
            cx.notify();
        }
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
        // An activity-only event: no toast, the token still taken.
        assert!(!followed.take(&toasts.toasts, false).1);
        toasts.push("Saved.", ToastTone::Info, None, Vec::new());
        let (fresh, draw) = followed.take(&toasts.toasts, false);
        assert_eq!(fresh.len(), 1);
        assert!(draw);
        // The same toast is not news twice.
        assert!(!followed.take(&toasts.toasts, false).1);
        assert!(followed.take(&toasts.toasts, true).1);
        assert!(!followed.take(&toasts.toasts, true).1);
    }
}
