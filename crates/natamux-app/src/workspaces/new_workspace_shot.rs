//! Fills the Start a task dialog for its `--shot` states (T-0292, layout of T-0305): files and
//! an edited branch, no worktree, One agent with no agent, and the daemon's error. Shots only.

use std::path::PathBuf;

use gpui::{point, px};
use ui::prelude::*;

use super::super::new_workspace_rules::PickedFile;
use super::NewWorkspaceDialog;

/// The title of the filled shots: it starts with a task id, so it names the branch.
const TITLE: &str = "ABC-123 Fix the login redirect";
const NOTES: &str = "After sign-in the app goes to /home, not back to the page the user was on.";
/// The branch typed by hand in the filled shot: the field shows the changed dot and reset.
const OWN_BRANCH: &str = "fix/login-redirect";
const ERROR: &str = "the branch natamux/ABC-123-fix-the-login-redirect already exists.";
/// The files of the filled shot; their paths are read only when the task starts.
const FILES: [(&str, u64); 2] = [("login-redirect.png", 412_000), ("auth-server.log", 12_400)];

/// What a shot puts in the dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartShot {
    /// Title, notes, two files and a branch typed by hand.
    Filled,
    /// A title, and Create a worktree off.
    NoWorktree,
    /// A title, One agent and no agent: the Task flow row shows its error.
    OneAgentNoAgent,
    /// A title and the daemon's answer to a failed start.
    Error,
}

impl NewWorkspaceDialog {
    pub fn fill_for_shot(&mut self, shot: StartShot, window: &mut Window, cx: &mut Context<Self>) {
        self.title
            .update(cx, |editor, cx| editor.set_text(TITLE, window, cx));
        match shot {
            StartShot::Filled => {
                self.notes
                    .update(cx, |editor, cx| editor.set_text(NOTES, window, cx));
                for (name, size) in FILES {
                    self.next_file += 1;
                    self.files.push(PickedFile {
                        id: self.next_file,
                        name: name.into(),
                        size,
                        path: PathBuf::from(name),
                    });
                }
                self.branch_name
                    .update(cx, |editor, cx| editor.set_text(OWN_BRANCH, window, cx));
                self.branch_edited = true;
            }
            StartShot::NoWorktree => self.worktree = false,
            StartShot::OneAgentNoAgent => {
                self.single_agent = true;
                self.agent.clear();
            }
            StartShot::Error => self.error = Some(ERROR.into()),
        }
        cx.notify();
    }

    /// Scrolls the body to its end. Since T-0305 the body fits 1024x768, so this moves it only in
    /// a smaller window.
    pub fn scroll_to_end(&mut self, cx: &mut Context<Self>) {
        let end = self.scroll.max_offset().y;
        self.scroll.set_offset(point(px(0.), -end));
        cx.notify();
    }
}
