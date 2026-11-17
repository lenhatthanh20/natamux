//! Annotate with Git Blame for the file editor and the diff view: the right-click menu of each
//! editor, the blame request, and the labels drawn as inlays at the start of each line. Inlays
//! need no project (Zed's own `GitBlame` needs a project and a repository) and move with the
//! text, the excerpts and the empty rows of side by side.

use std::rc::Rc;
use std::sync::Arc;

use gpui::{ClipboardItem, Entity, EventEmitter, WeakEntity};
use natamux_proto::workspaces::changes::{BlameAnswer, ImageSide};
use ui::{ContextMenu, ContextMenuEntry, prelude::*};

use super::blame_rules::{BlameItem, NOT_TRACKED, commit_at, line_labels, menu_items};
use super::code_target::{BlameTarget, RowMenu};
use crate::app::source::{Source, reason};

/// True while the file has unsaved edits: blame reads the file on disk.
pub type Blocked = Box<dyn Fn(&App) -> bool>;

/// Why the column could not show; the owner shows it as a toast.
pub struct BlameFailed(pub String);

/// One text that can show the column, and what it shows.
struct Side {
    target: Rc<dyn BlameTarget>,
    side: ImageSide,
    answer: Option<BlameAnswer>,
}

pub struct BlameMarks {
    source: Option<Arc<Source>>,
    task_id: String,
    path: String,
    on: bool,
    sides: Vec<Side>,
    blocked: Option<Blocked>,
    /// Drops an answer when a newer request, or Close, came after it.
    asked: u64,
}

impl EventEmitter<BlameFailed> for BlameMarks {}

impl BlameMarks {
    pub fn new(blocked: Option<Blocked>) -> Self {
        BlameMarks {
            source: None,
            task_id: String::new(),
            path: String::new(),
            on: false,
            sides: Vec::new(),
            blocked,
            asked: 0,
        }
    }

    /// The editors of `path` now. Another file starts with the column off; the same file asks
    /// again for the new editors or text while it is on.
    pub fn attach(
        &mut self,
        source: Arc<Source>,
        (task_id, path): (&str, &str),
        sides: Vec<(Rc<dyn BlameTarget>, ImageSide)>,
        cx: &mut Context<Self>,
    ) {
        // Nothing attached yet counts as the same file: a shot may turn it on first.
        let same_file = self.path.is_empty() || (self.task_id == task_id && self.path == path);
        self.clear(cx);
        self.source = Some(source);
        self.task_id = task_id.to_owned();
        self.path = path.to_owned();
        let marks = cx.weak_entity();
        self.sides = sides
            .into_iter()
            .enumerate()
            .map(|(index, (target, side))| {
                set_menu(target.as_ref(), index, marks.clone(), cx);
                Side {
                    target,
                    side,
                    answer: None,
                }
            })
            .collect();
        if same_file && self.on {
            self.ask(cx);
        } else {
            self.on = false;
        }
    }

    /// A newer text of the same editors.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.on {
            self.ask(cx);
        }
    }

    /// The menu's Annotate with Git Blame, for the shots.
    pub fn annotate(&mut self, cx: &mut Context<Self>) {
        self.turn(true, cx);
    }

    fn turn(&mut self, on: bool, cx: &mut Context<Self>) {
        self.on = on;
        if on {
            self.ask(cx);
        } else {
            self.asked += 1;
            self.clear(cx);
        }
        cx.notify();
    }

    fn ask(&mut self, cx: &mut Context<Self>) {
        let Some(source) = self.source.clone() else {
            return;
        };
        self.asked += 1;
        let asked = self.asked;
        for (index, side) in self.sides.iter().enumerate() {
            let answer = source.blame(&self.task_id, &self.path, side.side);
            cx.spawn(async move |this, cx| {
                let answer = answer.await;
                this.update(cx, |this, cx| {
                    if this.asked == asked {
                        this.got(index, answer.map_err(|error| reason(&error)), cx);
                    }
                })
                .ok();
            })
            .detach();
        }
    }

    fn got(&mut self, index: usize, answer: Result<BlameAnswer, String>, cx: &mut Context<Self>) {
        let modified = self
            .sides
            .get(index)
            .is_some_and(|side| side.side == ImageSide::Modified);
        match answer {
            Err(error) => {
                cx.emit(BlameFailed(error));
                self.turn(false, cx);
            }
            Ok(answer) if !answer.tracked && modified => {
                cx.emit(BlameFailed(NOT_TRACKED.into()));
                self.turn(false, cx);
            }
            Ok(answer) => {
                let Some(side) = self.sides.get_mut(index) else {
                    return;
                };
                let lines = side.target.lines(cx);
                side.target.set_labels(line_labels(&answer, lines), cx);
                side.answer = Some(answer);
            }
        }
    }

    /// Takes every label away.
    fn clear(&mut self, cx: &mut App) {
        for side in &mut self.sides {
            side.answer = None;
            side.target.clear_labels(cx);
        }
    }

    fn items(&self, index: usize, row: Option<u32>, cx: &App) -> Vec<BlameItem> {
        let blocked = self.blocked.as_ref().is_some_and(|blocked| blocked(cx));
        let answer = self.sides.get(index).and_then(|side| side.answer.as_ref());
        let commit = answer
            .zip(row)
            .and_then(|(answer, row)| commit_at(answer, row));
        menu_items(self.on, blocked, commit)
    }

    /// The menu of side `index` at `row`, as a right click there would build it.
    #[cfg(test)]
    pub(crate) fn menu_for(&self, index: usize, row: Option<u32>, cx: &App) -> Vec<BlameItem> {
        self.items(index, row, cx)
    }
}

/// The right-click menu of side `index`.
fn set_menu(target: &dyn BlameTarget, index: usize, marks: WeakEntity<BlameMarks>, cx: &mut App) {
    let menu: RowMenu = Rc::new(
        move |row: Option<u32>, window: &mut Window, cx: &mut App| -> Option<Entity<ContextMenu>> {
            let items = marks.upgrade()?.read(cx).items(index, row, cx);
            let marks = marks.clone();
            Some(ContextMenu::build(window, cx, move |menu, _, _| {
                fill(menu, items, marks)
            }))
        },
    );
    target.set_menu(menu, cx);
}

fn fill(
    mut menu: ContextMenu,
    items: Vec<BlameItem>,
    marks: WeakEntity<BlameMarks>,
) -> ContextMenu {
    for item in items {
        let marks = marks.clone();
        let turn = move |on: bool, cx: &mut App| {
            marks.update(cx, |marks, cx| marks.turn(on, cx)).ok();
        };
        menu = match item {
            BlameItem::Annotate { blocked: true } => menu
                .item(ContextMenuEntry::new("Annotate with Git Blame (save first)").disabled(true)),
            BlameItem::Annotate { blocked: false } => {
                menu.entry("Annotate with Git Blame", None, move |_, cx| turn(true, cx))
            }
            BlameItem::Title(title) => menu.label(title),
            BlameItem::CopyHash(sha) => menu.entry("Copy Commit Hash", None, move |_, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(sha.clone()))
            }),
            BlameItem::Close => menu.entry("Close Annotations", None, move |_, cx| turn(false, cx)),
        };
    }
    menu
}
