//! Browse… in the Projects dialog: the system folder picker for the Folder and the Worktree
//! folder fields (T-0225).

use std::path::PathBuf;

use gpui::{Focusable, PathPromptOptions};
use ui::{Tooltip, prelude::*};

use super::ProjectsDialog;
use crate::components::button_style::action_button;

/// The field a Browse… picker fills.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Pick {
    Folder,
    Worktree,
}

impl ProjectsDialog {
    /// Browse…: the system folder picker for one field. A cancel does nothing.
    fn browse(&mut self, pick: Pick, window: &mut Window, cx: &mut Context<Self>) {
        if self.picking.is_some() || self.busy {
            return;
        }
        self.picking = Some(pick);
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: None,
        });
        cx.spawn_in(window, async move |this, cx| {
            let picked = match paths.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                _ => None,
            };
            this.update_in(cx, |this, window, cx| {
                this.took_folder(pick, picked, window, cx)
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// The picker's answer: the folder goes in its field, as if typed and left.
    pub(super) fn took_folder(
        &mut self,
        pick: Pick,
        path: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.picking = None;
        cx.notify();
        let Some(path) = path else {
            return;
        };
        let path = path.to_string_lossy().into_owned();
        let editor = match pick {
            Pick::Folder => {
                self.folder_error = None;
                self.folder_touched = true;
                self.folder.clone()
            }
            Pick::Worktree => {
                // A pick is the user's own value: a new daemon default does not replace it.
                self.typed_worktree = Some(path.clone());
                self.worktree_error = None;
                self.worktree_touched = true;
                self.worktree.clone()
            }
        };
        editor.update(cx, |editor, cx| editor.set_text(path, window, cx));
        window.focus(&editor.focus_handle(cx), cx);
    }

    pub(super) fn browse_button(&self, pick: Pick, cx: &mut Context<Self>) -> Button {
        let (id, tip, focus) = match pick {
            Pick::Folder => (
                "projects-browse-folder",
                "Choose the folder of a git repository",
                &self.folder_browse,
            ),
            Pick::Worktree => (
                "projects-browse-worktree",
                "Choose the folder for the worktrees",
                &self.worktree_browse,
            ),
        };
        action_button(id, "Browse\u{2026}")
            .track_focus(focus)
            .disabled(self.busy || self.picking.is_some())
            .tooltip(Tooltip::text(tip))
            .on_click(cx.listener(move |this, _, window, cx| this.browse(pick, window, cx)))
    }
}
