//! What the app shell says, with no gpui: the window title, the title bar's breadcrumb and
//! needs-you pill, and the status bar's items (`features/app/TitleBar.tsx`, `StatusBar.tsx` and
//! the document title of `Dashboard.tsx`).

use natamux_proto::projects::repo::Repo;
use natamux_proto::tasks::task::{Task, TaskState};

use super::dashboard_data::Link;
use super::layout_mode::BREAKPOINT_LG;

/// "1 needs you", "3 need you".
pub fn needs_you_text(count: usize) -> String {
    let verb = if count == 1 { "needs" } else { "need" };
    format!("{count} {verb} you")
}

/// "(3) Natamux" while tasks wait for the user, else "Natamux".
pub fn window_title(needs_you: usize) -> String {
    if needs_you > 0 {
        format!("({needs_you}) Natamux")
    } else {
        "Natamux".into()
    }
}

pub fn running_count(tasks: &[Task]) -> usize {
    tasks
        .iter()
        .filter(|task| task.state == TaskState::Running)
        .count()
}

/// The first status item: the dot, its word and its tooltip.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveItem {
    pub label: &'static str,
    pub tip: String,
    /// The green dot; false shows the signal dot.
    pub ok: bool,
}

pub fn live_item(link: &Link) -> LiveItem {
    match link {
        Link::Live => LiveItem {
            label: "Live",
            tip: "Changes appear as they happen".into(),
            ok: true,
        },
        Link::Loading => LiveItem {
            label: "Connecting",
            tip: "Connecting to the daemon".into(),
            ok: false,
        },
        Link::Down {
            retrying: true,
            reason,
        } => LiveItem {
            label: "Reconnecting",
            tip: format!("Lost the connection. Trying to reconnect. {reason}"),
            ok: false,
        },
        // The web has no such state: it signs out instead. The app says why it gave up.
        Link::Down {
            retrying: false,
            reason,
        } => LiveItem {
            label: "Not connected",
            tip: reason.clone(),
            ok: false,
        },
    }
}

/// The title bar's "where you are": a view name, or `repo / T-0018 title` for an open task.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Crumb {
    View(&'static str),
    Task {
        repo: Option<String>,
        id: String,
        title: Option<String>,
    },
}

/// Under 1024 the wordmark goes and the crumb of a task is its id only.
pub fn is_wide(width: f32) -> bool {
    width >= BREAKPOINT_LG
}

pub fn crumb(open: Option<&Task>, agents: bool, repos: &[Repo], width: f32) -> Crumb {
    let Some(task) = open else {
        return Crumb::View(if agents { "Agents" } else { "All tasks" });
    };
    if !is_wide(width) {
        return Crumb::Task {
            repo: None,
            id: task.id.clone(),
            title: None,
        };
    }
    let repo_id = task.repo.as_deref().unwrap_or("default");
    let repo = repos
        .iter()
        .find(|repo| repo.id == repo_id)
        .map(|repo| repo.name.clone())
        .or_else(|| task.repo.clone());
    Crumb::Task {
        repo,
        id: task.id.clone(),
        title: Some(task.title.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tasks::task_attention::needs_me_tasks;

    fn task(id: &str, state: TaskState, updated_at: &str) -> Task {
        let mut task: Task = serde_json::from_value(serde_json::json!({
            "id": id,
            "title": format!("Title of {id}"),
            "state": "draft",
            "createdAt": "2026-10-22T09:00:00.000Z",
            "updatedAt": updated_at,
        }))
        .expect("a task");
        task.state = state;
        task
    }

    fn repo(id: &str, name: &str) -> Repo {
        Repo {
            id: id.into(),
            name: name.into(),
            path: format!("/src/{name}"),
            worktrees_dir: None,
        }
    }

    const NOW: i64 = 1_792_661_400_000; // 2026-10-22T09:30:00Z

    #[test]
    fn says_need_or_needs_by_the_count() {
        assert_eq!(needs_you_text(1), "1 needs you");
        assert_eq!(needs_you_text(3), "3 need you");
    }

    #[test]
    fn puts_the_count_in_the_window_title_only_while_tasks_wait() {
        assert_eq!(window_title(0), "Natamux");
        assert_eq!(window_title(2), "(2) Natamux");
    }

    #[test]
    fn the_pill_filters_to_every_task_that_needs_you() {
        let tasks = [
            task("T-1", TaskState::Running, "2026-10-22T09:29:00.000Z"),
            task("T-2", TaskState::Review, "2026-10-22T09:20:00.000Z"),
            task("T-3", TaskState::NeedsInput, "2026-10-22T09:10:00.000Z"),
            task("T-4", TaskState::Running, "2026-10-22T09:00:00.000Z"),
            task("T-5", TaskState::Failed, "2026-10-22T09:25:00.000Z"),
        ];
        let ids: Vec<String> = needs_me_tasks(&tasks, NOW)
            .into_iter()
            .map(|task| task.id)
            .collect();
        assert_eq!(ids, ["T-4", "T-3", "T-2", "T-5"]);
    }

    #[test]
    fn counts_the_running_tasks() {
        let tasks = [
            task("T-1", TaskState::Running, "2026-10-22T09:29:00.000Z"),
            task("T-2", TaskState::Done, "2026-10-22T09:29:00.000Z"),
            task("T-3", TaskState::Running, "2026-10-22T09:29:00.000Z"),
        ];
        assert_eq!(running_count(&tasks), 2);
    }

    #[test]
    fn names_each_link_state() {
        assert_eq!(live_item(&Link::Live).label, "Live");
        assert!(live_item(&Link::Live).ok);
        assert_eq!(live_item(&Link::Loading).label, "Connecting");
        let retrying = live_item(&Link::Down {
            reason: "connection refused".into(),
            retrying: true,
        });
        assert_eq!(retrying.label, "Reconnecting");
        assert!(!retrying.ok);
        let refused = live_item(&Link::Down {
            reason: "The daemon refused the token.".into(),
            retrying: false,
        });
        assert_eq!(refused.label, "Not connected");
        assert_eq!(refused.tip, "The daemon refused the token.");
    }

    #[test]
    fn the_crumb_is_the_view_name_without_an_open_task() {
        assert_eq!(crumb(None, false, &[], 1280.0), Crumb::View("All tasks"));
        assert_eq!(crumb(None, true, &[], 1280.0), Crumb::View("Agents"));
    }

    #[test]
    fn the_crumb_of_a_task_names_its_project_and_title_when_wide() {
        let mut open = task("T-0018", TaskState::Running, "2026-10-22T09:29:00.000Z");
        open.repo = Some("r1".into());
        let repos = [repo("r1", "natamux")];
        assert_eq!(
            crumb(Some(&open), false, &repos, 1280.0),
            Crumb::Task {
                repo: Some("natamux".into()),
                id: "T-0018".into(),
                title: Some("Title of T-0018".into()),
            }
        );
        // An unknown project shows its id; a task with none belongs to the default project.
        open.repo = Some("gone".into());
        assert!(matches!(
            crumb(Some(&open), false, &repos, 1280.0),
            Crumb::Task { repo: Some(name), .. } if name == "gone"
        ));
        open.repo = None;
        let repos = [repo("default", "main project")];
        assert!(matches!(
            crumb(Some(&open), false, &repos, 1280.0),
            Crumb::Task { repo: Some(name), .. } if name == "main project"
        ));
    }

    #[test]
    fn the_crumb_of_a_task_is_its_id_under_1024() {
        let open = task("T-0018", TaskState::Running, "2026-10-22T09:29:00.000Z");
        assert_eq!(
            crumb(Some(&open), false, &[], 1000.0),
            Crumb::Task {
                repo: None,
                id: "T-0018".into(),
                title: None,
            }
        );
        assert!(!is_wide(1023.0));
        assert!(is_wide(1024.0));
    }
}
