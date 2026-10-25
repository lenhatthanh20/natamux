//! The icons of the pipeline editor (`KIND_ICON` of `PipelinePreview.tsx`, the marks and the
//! row parts of `StageList.tsx`), in one place: Zed's icons, the nearest to the web's Lucide ones
//! (docs/rust-phase3.md, "Screen 5: where it differs").

use crate::components::icons::icon_path;
use natamux_proto::stages::schema::StageKind;
use ui::prelude::*;

fn icon(name: &str) -> Icon {
    Icon::from_path(icon_path(name))
}

/// context `BookOpen`, agent `Bot`, review `Eye`, testing `FlaskConical`, release `Rocket`,
/// manual `Hand`.
pub fn kind_icon(kind: StageKind) -> Icon {
    match kind {
        StageKind::Context => Icon::new(IconName::Book),
        StageKind::Agent => icon("bot"),
        StageKind::Review => icon("eye"),
        StageKind::Testing => Icon::new(IconName::ToolHammer),
        StageKind::Release => Icon::new(IconName::PullRequest),
        StageKind::Manual => Icon::new(IconName::Person),
    }
}

/// `UserRound`: the stage waits for a person.
pub fn waits_icon() -> Icon {
    Icon::new(IconName::Person)
}

/// `Pencil`: the stage has a prompt of its own.
pub fn prompt_icon() -> Icon {
    icon("pencil")
}

/// `GitFork`: a branch.
pub fn branch_icon() -> Icon {
    Icon::new(IconName::GitBranch)
}

/// `GripVertical`: the drag handle of a row.
pub fn grip_icon() -> Icon {
    Icon::new(IconName::Menu)
}

/// `Trash2`: Remove.
pub fn remove_icon() -> Icon {
    icon("trash")
}

/// `CircleAlert`: a problem under a row or a field.
pub fn problem_icon() -> Icon {
    icon("circle-alert")
}
