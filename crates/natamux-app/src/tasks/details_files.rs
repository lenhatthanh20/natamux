//! The Files tab of the panel (`FileTree.tsx`): one tree per task shown, made when it first
//! shows. It reads again when the workspace changes only while it shows (`dirty_reads`), and
//! when it shows again.

use ui::prelude::*;

use super::DetailsPanel;
use crate::app::tab_rules::dirty_reads;
use crate::files::file_tree::FileTree;

impl DetailsPanel {
    pub(super) fn files_body(
        &mut self,
        task_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let shows_again = self.tree_shown.draw();
        let tree = match &self.files {
            Some(tree) if tree.read(cx).task_id() == task_id => {
                if shows_again {
                    tree.update(cx, |tree, cx| tree.refresh(cx));
                }
                tree.clone()
            }
            _ => {
                let (dashboard, id) = (self.dashboard.clone(), task_id.to_owned());
                let tree = cx.new(|cx| FileTree::new(dashboard, id, window, cx));
                self.files = Some(tree.clone());
                tree
            }
        };
        tree.into_any_element()
    }

    /// `workspace.dirty` and the fallback poll: the shown folders and the changes again, while
    /// the panel shows Files.
    pub(super) fn refresh_tree(&mut self, cx: &mut Context<Self>) {
        let shown = (self.is_open(cx) && self.edit.is_none()).then(|| self.view(cx));
        if !dirty_reads(None, shown).tree {
            return;
        }
        if let Some(tree) = &self.files {
            tree.update(cx, |tree, cx| tree.refresh(cx));
        }
    }
}
