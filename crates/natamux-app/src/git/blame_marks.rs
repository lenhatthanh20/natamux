//! Annotate with Git Blame for the file editor and the diff view: the right-click menu of each
//! editor, the blame request, and the labels drawn as inlays at the start of each line. Inlays
//! need no project (Zed's own `GitBlame` needs a project and a repository) and move with the
//! text, the excerpts and the empty rows of side by side.

use std::sync::Arc;

use editor::{DisplayPoint, Editor, Inlay};
use gpui::{ClipboardItem, Entity, EventEmitter, WeakEntity};
use language::Buffer;
use natamux_proto::workspaces::changes::{BlameAnswer, ImageSide};
use text::Point;
use ui::{ContextMenu, ContextMenuEntry, prelude::*};

use super::blame_rules::{BlameItem, NOT_TRACKED, commit_at, line_labels, menu_items};
use crate::app::source::{Source, reason};

/// True while the file has unsaved edits: blame reads the file on disk.
pub type Blocked = Box<dyn Fn(&App) -> bool>;

/// Why the column could not show; the owner shows it as a toast.
pub struct BlameFailed(pub String);

/// One editor that can show the column, and what it shows.
struct Side {
    editor: WeakEntity<Editor>,
    buffer: Entity<Buffer>,
    side: ImageSide,
    answer: Option<BlameAnswer>,
    inlays: Vec<Inlay>,
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
    /// Inlay ids far above the editor's own, so they never meet.
    next_id: usize,
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
            next_id: usize::MAX / 2,
        }
    }

    /// The editors of `path` now. Another file starts with the column off; the same file asks
    /// again for the new editors or text while it is on.
    pub fn attach(
        &mut self,
        source: Arc<Source>,
        (task_id, path): (&str, &str),
        editors: Vec<(Entity<Editor>, Entity<Buffer>, ImageSide)>,
        cx: &mut Context<Self>,
    ) {
        // Nothing attached yet counts as the same file: a shot may turn it on first.
        let same_file = self.path.is_empty() || (self.task_id == task_id && self.path == path);
        self.clear(cx);
        self.source = Some(source);
        self.task_id = task_id.to_owned();
        self.path = path.to_owned();
        let marks = cx.weak_entity();
        self.sides = editors
            .into_iter()
            .enumerate()
            .map(|(index, (editor, buffer, side))| {
                set_menu(&editor, &buffer, index, marks.clone(), cx);
                Side {
                    editor: editor.downgrade(),
                    buffer,
                    side,
                    answer: None,
                    inlays: Vec::new(),
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
                let first_id = self.next_id;
                let Some(side) = self.sides.get_mut(index) else {
                    return;
                };
                let added = show(side, &answer, first_id, cx);
                self.next_id = first_id.wrapping_add(added);
                side.answer = Some(answer);
            }
        }
    }

    /// Takes every label away.
    fn clear(&mut self, cx: &mut App) {
        for side in &mut self.sides {
            side.answer = None;
            let ids: Vec<_> = side.inlays.drain(..).map(|inlay| inlay.id).collect();
            if let Some(editor) = side.editor.upgrade() {
                editor.update(cx, |editor, cx| editor.splice_inlays(&ids, Vec::new(), cx));
            }
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
}

/// Draws the labels of `answer` into the side's editor, in place of the ones it had. Gives how
/// many ids it may have used.
fn show(side: &mut Side, answer: &BlameAnswer, first_id: usize, cx: &mut App) -> usize {
    let Some(editor) = side.editor.upgrade() else {
        return 0;
    };
    let lines = side.buffer.read(cx).max_point().row + 1;
    let labels = line_labels(answer, lines);
    let old: Vec<_> = side.inlays.drain(..).map(|inlay| inlay.id).collect();
    let buffer = side.buffer.clone();
    let inlays: Vec<Inlay> = editor.update(cx, |editor, cx| {
        let multi = editor.buffer().read(cx);
        let inlays = labels
            .into_iter()
            .enumerate()
            .filter_map(|(at, (line, label))| {
                // A line outside every excerpt (collapsed) has no anchor and no label.
                let anchor = multi.buffer_point_to_anchor(&buffer, Point::new(line, 0), cx)?;
                Some(Inlay::debugger(first_id.wrapping_add(at), anchor, label))
            })
            .collect::<Vec<_>>();
        editor.splice_inlays(&old, inlays.clone(), cx);
        inlays
    });
    side.inlays = inlays;
    lines as usize
}

/// The line of `buffer` at a clicked row; None for a row of another buffer (a deleted line of
/// an inline diff).
fn buffer_row(
    editor: &mut Editor,
    point: DisplayPoint,
    buffer: &Entity<Buffer>,
    cx: &mut Context<Editor>,
) -> Option<u32> {
    let snapshot = editor.display_snapshot(cx);
    let point = point.to_point(&snapshot);
    let multi = editor.buffer().read(cx).snapshot(cx);
    let (found, point) = multi.point_to_buffer_point(point)?;
    (found.remote_id() == buffer.read(cx).remote_id()).then_some(point.row)
}

/// The right-click menu of the text. Line numbers get it once the Zed fork opens a custom menu
/// from the gutter of an editor with no project.
fn set_menu(
    editor: &Entity<Editor>,
    buffer: &Entity<Buffer>,
    index: usize,
    marks: WeakEntity<BlameMarks>,
    cx: &mut App,
) {
    let buffer = buffer.clone();
    editor.update(cx, |editor, _| {
        editor.set_custom_context_menu(move |editor, point, window, cx| {
            let row = buffer_row(editor, point, &buffer, cx);
            let items = marks.upgrade()?.read(cx).items(index, row, cx);
            let marks = marks.clone();
            Some(ContextMenu::build(window, cx, move |menu, _, _| {
                fill(menu, items, marks)
            }))
        });
    });
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
