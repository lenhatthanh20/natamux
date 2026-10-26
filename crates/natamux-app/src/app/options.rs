//! Command line flags. CI uses them to check the app and take screenshots.

pub const USAGE: &str =
    "usage: natamux-app [--smoke] [--theme light|dark] [--size WxH] [--hold SECONDS] [--fake-daemon]
                   [--gate2 TASK] [--shot STATE] [--count-draws] [--open-task TASK]
  --smoke   draw a frame, type into the text box, check it, then quit (exit 1 on failure)
            with --fake-daemon, also check that a workspace's terminal grid does not scroll
            and that the pipeline chart scrolls to its last step
  --theme   use this theme instead of following the system
  --size    window size in points, placed at the top left of the screen
  --hold    quit after this many seconds (for screenshots)
  --fake-daemon  show fixed tasks and a fixed terminal instead of the daemon's (for screenshots)
  --gate2   run gate 2 on TASK's shells 901-912, save the perf report, print its path, then quit
  --shot    open the window in one screen-1 or screen-2 state, for the CI screenshots (docs/rust-phase3.md)
  --count-draws  print the window's draws once a second, to measure the cost of motion
  --open-task    open TASK once the dashboard has it, as a click on its row does

The daemon: NATAMUX_URL and NATAMUX_TOKEN, else http://127.0.0.1:4317 and the Natamux app's token.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeMode {
    Light,
    Dark,
}

/// The screen-1 to screen-7 states CI shoots (`--shot`). `app/shots.rs` puts the window in each one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shot {
    TasksTable,
    TasksBoard,
    BoardHoverCard,
    AddTaskDialog,
    StartTask,
    RemoveWorkspace,
    ClearFinished,
    AddSubtask,
    TaskContextMenu,
    Skeleton,
    Toast,
    Empty,
    FilterMiss,
    NoProjects,
    LoadError,
    Drawer,
    WorkspaceTerminalRunning,
    TerminalFind,
    WorkspaceTerminalNeedsInput,
    WorkspaceTerminalReview,
    NoWorkspace,
    WorkspacePreparing,
    FinishedWrapUp,
    AgentsGrid,
    TaskDetailsReview,
    TaskDetailsNeedsInput,
    AnswerBanner,
    WorkspaceTerminalPlanGate,
    WorkspaceProgress,
    StageFailed,
    SendBack,
    DetailsDetails,
    CompareTries,
    DetailsMoreMenu,
    FanOut,
    WorkspaceChangesEmpty,
    WorkspaceChangesList,
    WorkspaceChangesDiff,
    WorkspaceChangesAsk,
    Rollback,
    ImageDiff,
    WorkspaceHistory,
    TasksProgress,
    ProjectSettingsTabStages,
    ProjectSettingsAgentStage,
    ProjectSettingsStagePicker,
    ProjectSettingsBranch,
    ProjectSettingsProblems,
    ProjectSettingsConflict,
    DetailsFiles,
    WorkspaceDocs,
    WorkspaceDocsEdit,
    WorkspaceEditor,
    WorkspaceEditorSource,
    WorkspaceEditorDraft,
    WorkspaceEditorConflict,
    TerminalDropTarget,
    Palette,
    PaletteQuery,
    PaletteNoMatch,
    PaletteJump,
    Shortcuts,
    AccountMenu,
    Settings,
    SettingsGeneral,
    SettingsSearch,
    Projects,
    ProjectSettings,
    ProjectSettingsTabIssues,
    ProjectSettingsTabJson,
    ProjectSettingsJsonError,
    ImportIssues,
    SignIn,
    SignInError,
    AddMenu,
    DetailsEmpty,
    RailClosedPanel,
    ChangesOverMain,
    StartTaskBranchMenu,
}

impl Shot {
    pub const ALL: [Shot; 79] = [
        Shot::TasksTable,
        Shot::TasksBoard,
        Shot::BoardHoverCard,
        Shot::AddTaskDialog,
        Shot::StartTask,
        Shot::RemoveWorkspace,
        Shot::ClearFinished,
        Shot::AddSubtask,
        Shot::TaskContextMenu,
        Shot::Skeleton,
        Shot::Toast,
        Shot::Empty,
        Shot::FilterMiss,
        Shot::NoProjects,
        Shot::LoadError,
        Shot::Drawer,
        Shot::WorkspaceTerminalRunning,
        Shot::TerminalFind,
        Shot::WorkspaceTerminalNeedsInput,
        Shot::WorkspaceTerminalReview,
        Shot::NoWorkspace,
        Shot::WorkspacePreparing,
        Shot::FinishedWrapUp,
        Shot::AgentsGrid,
        Shot::TaskDetailsReview,
        Shot::TaskDetailsNeedsInput,
        Shot::AnswerBanner,
        Shot::WorkspaceTerminalPlanGate,
        Shot::WorkspaceProgress,
        Shot::StageFailed,
        Shot::SendBack,
        Shot::DetailsDetails,
        Shot::CompareTries,
        Shot::DetailsMoreMenu,
        Shot::FanOut,
        Shot::WorkspaceChangesEmpty,
        Shot::WorkspaceChangesList,
        Shot::WorkspaceChangesDiff,
        Shot::WorkspaceChangesAsk,
        Shot::Rollback,
        Shot::ImageDiff,
        Shot::WorkspaceHistory,
        Shot::TasksProgress,
        Shot::ProjectSettingsTabStages,
        Shot::ProjectSettingsAgentStage,
        Shot::ProjectSettingsStagePicker,
        Shot::ProjectSettingsBranch,
        Shot::ProjectSettingsProblems,
        Shot::ProjectSettingsConflict,
        Shot::DetailsFiles,
        Shot::WorkspaceDocs,
        Shot::WorkspaceDocsEdit,
        Shot::WorkspaceEditor,
        Shot::WorkspaceEditorSource,
        Shot::WorkspaceEditorDraft,
        Shot::WorkspaceEditorConflict,
        Shot::TerminalDropTarget,
        Shot::Palette,
        Shot::PaletteQuery,
        Shot::PaletteNoMatch,
        Shot::PaletteJump,
        Shot::Shortcuts,
        Shot::AccountMenu,
        Shot::Settings,
        Shot::SettingsGeneral,
        Shot::SettingsSearch,
        Shot::Projects,
        Shot::ProjectSettings,
        Shot::ProjectSettingsTabIssues,
        Shot::ProjectSettingsTabJson,
        Shot::ProjectSettingsJsonError,
        Shot::ImportIssues,
        Shot::SignIn,
        Shot::SignInError,
        Shot::AddMenu,
        Shot::DetailsEmpty,
        Shot::RailClosedPanel,
        Shot::ChangesOverMain,
        Shot::StartTaskBranchMenu,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Shot::TasksTable => "tasks-table",
            Shot::TasksBoard => "tasks-board",
            Shot::BoardHoverCard => "board-hover-card",
            Shot::AddTaskDialog => "add-task-dialog",
            Shot::StartTask => "start-task",
            Shot::RemoveWorkspace => "remove-workspace",
            Shot::ClearFinished => "clear-finished",
            Shot::AddSubtask => "add-subtask",
            Shot::TaskContextMenu => "task-context-menu",
            Shot::Skeleton => "skeleton",
            Shot::Toast => "toast",
            Shot::Empty => "empty",
            Shot::FilterMiss => "filter-miss",
            Shot::NoProjects => "no-projects",
            Shot::LoadError => "load-error",
            Shot::Drawer => "drawer",
            Shot::WorkspaceTerminalRunning => "workspace-terminal-running",
            Shot::TerminalFind => "terminal-find",
            Shot::WorkspaceTerminalNeedsInput => "workspace-terminal-needs-input",
            Shot::WorkspaceTerminalReview => "workspace-terminal-review",
            Shot::NoWorkspace => "no-workspace",
            Shot::WorkspacePreparing => "workspace-preparing",
            Shot::FinishedWrapUp => "finished-wrap-up",
            Shot::AgentsGrid => "agents-grid",
            Shot::TaskDetailsReview => "task-details-review",
            Shot::TaskDetailsNeedsInput => "task-details-needs-input",
            Shot::AnswerBanner => "answer-banner",
            Shot::WorkspaceTerminalPlanGate => "workspace-terminal-plan-gate",
            Shot::WorkspaceProgress => "workspace-progress",
            Shot::StageFailed => "stage-failed",
            Shot::SendBack => "send-back",
            Shot::DetailsDetails => "details-details",
            Shot::CompareTries => "compare-tries",
            Shot::DetailsMoreMenu => "details-more-menu",
            Shot::FanOut => "fan-out",
            Shot::WorkspaceChangesEmpty => "workspace-changes-empty",
            Shot::WorkspaceChangesList => "workspace-changes-list",
            Shot::WorkspaceChangesDiff => "workspace-changes-diff",
            Shot::WorkspaceChangesAsk => "workspace-changes-ask",
            Shot::Rollback => "rollback",
            Shot::ImageDiff => "image-diff",
            Shot::WorkspaceHistory => "workspace-history",
            Shot::TasksProgress => "tasks-progress",
            Shot::ProjectSettingsTabStages => "project-settings-tab-stages",
            Shot::ProjectSettingsAgentStage => "project-settings-agent-stage",
            Shot::ProjectSettingsStagePicker => "project-settings-stage-picker",
            Shot::ProjectSettingsBranch => "project-settings-branch",
            Shot::ProjectSettingsProblems => "project-settings-problems",
            Shot::ProjectSettingsConflict => "project-settings-conflict",
            Shot::DetailsFiles => "details-files",
            Shot::WorkspaceDocs => "workspace-docs",
            Shot::WorkspaceDocsEdit => "workspace-docs-edit",
            Shot::WorkspaceEditor => "workspace-editor",
            Shot::WorkspaceEditorSource => "workspace-editor-source",
            Shot::WorkspaceEditorDraft => "workspace-editor-draft",
            Shot::WorkspaceEditorConflict => "workspace-editor-conflict",
            Shot::TerminalDropTarget => "terminal-drop-target",
            Shot::Palette => "palette",
            Shot::PaletteQuery => "palette-query",
            Shot::PaletteNoMatch => "palette-no-match",
            Shot::PaletteJump => "palette-jump",
            Shot::Shortcuts => "shortcuts",
            Shot::AccountMenu => "account-menu",
            Shot::Settings => "settings",
            Shot::SettingsGeneral => "settings-general",
            Shot::SettingsSearch => "settings-search",
            Shot::Projects => "projects",
            Shot::ProjectSettings => "project-settings",
            Shot::ProjectSettingsTabIssues => "project-settings-tab-issues",
            Shot::ProjectSettingsTabJson => "project-settings-tab-json",
            Shot::ProjectSettingsJsonError => "project-settings-json-error",
            Shot::ImportIssues => "import-issues",
            Shot::SignIn => "sign-in",
            Shot::SignInError => "sign-in-error",
            Shot::AddMenu => "add-menu",
            Shot::DetailsEmpty => "details-empty",
            Shot::RailClosedPanel => "rail-closed-panel",
            Shot::ChangesOverMain => "changes-over-main",
            Shot::StartTaskBranchMenu => "start-task-branch-menu",
        }
    }

    fn parse(text: &str) -> Result<Shot, String> {
        Shot::ALL
            .into_iter()
            .find(|shot| shot.name() == text)
            .ok_or_else(|| format!("--shot does not know the state {text:?}"))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    pub smoke: bool,
    pub theme: Option<ThemeMode>,
    pub size: Option<(u32, u32)>,
    pub hold_seconds: Option<u64>,
    pub fake_daemon: bool,
    pub gate2: Option<String>,
    pub shot: Option<Shot>,
    pub count_draws: bool,
    pub open_task: Option<String>,
}

impl Options {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Options, String> {
        let mut options = Options {
            smoke: false,
            theme: None,
            size: None,
            hold_seconds: None,
            fake_daemon: false,
            gate2: None,
            shot: None,
            count_draws: false,
            open_task: None,
        };
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--smoke" => options.smoke = true,
                "--fake-daemon" => options.fake_daemon = true,
                "--count-draws" => options.count_draws = true,
                "--theme" => options.theme = Some(parse_theme(&value(&mut args, &arg)?)?),
                "--size" => options.size = Some(parse_size(&value(&mut args, &arg)?)?),
                "--gate2" => options.gate2 = Some(value(&mut args, &arg)?),
                "--open-task" => options.open_task = Some(value(&mut args, &arg)?),
                "--shot" => options.shot = Some(Shot::parse(&value(&mut args, &arg)?)?),
                "--hold" => {
                    let seconds = value(&mut args, &arg)?;
                    let seconds = seconds
                        .parse()
                        .map_err(|_| format!("--hold takes whole seconds, not {seconds:?}"))?;
                    options.hold_seconds = Some(seconds);
                }
                _ => return Err(format!("unknown argument {arg:?}")),
            }
        }
        Ok(options)
    }
}

fn value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    args.next().ok_or_else(|| format!("{flag} needs a value"))
}

fn parse_theme(text: &str) -> Result<ThemeMode, String> {
    match text {
        "light" => Ok(ThemeMode::Light),
        "dark" => Ok(ThemeMode::Dark),
        _ => Err(format!("--theme is light or dark, not {text:?}")),
    }
}

fn parse_size(text: &str) -> Result<(u32, u32), String> {
    let bad = || format!("--size looks like 1280x800, not {text:?}");
    let (width, height) = text.split_once('x').ok_or_else(bad)?;
    let width: u32 = width.parse().map_err(|_| bad())?;
    let height: u32 = height.parse().map_err(|_| bad())?;
    if width < 200 || height < 200 {
        return Err(bad());
    }
    Ok((width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Options, String> {
        Options::parse(args.iter().map(|arg| arg.to_string()))
    }

    #[test]
    fn no_flags_follow_the_system() {
        let options = parse(&[]).unwrap();
        assert!(!options.smoke);
        assert_eq!(options.theme, None);
        assert_eq!(options.size, None);
        assert_eq!(options.hold_seconds, None);
        assert!(!options.fake_daemon);
        assert_eq!(options.gate2, None);
        assert_eq!(options.shot, None);
        assert!(!options.count_draws);
        assert_eq!(options.open_task, None);
    }

    #[test]
    fn reads_every_flag() {
        let options = parse(&[
            "--fake-daemon",
            "--smoke",
            "--theme",
            "light",
            "--size",
            "1600x1000",
            "--hold",
            "12",
            "--gate2",
            "T-0145",
            "--shot",
            "task-context-menu",
            "--count-draws",
            "--open-task",
            "T-0146",
        ])
        .unwrap();
        assert!(options.count_draws);
        assert_eq!(options.open_task.as_deref(), Some("T-0146"));
        assert_eq!(options.shot, Some(Shot::TaskContextMenu));
        assert_eq!(options.gate2.as_deref(), Some("T-0145"));
        assert!(options.smoke);
        assert_eq!(options.theme, Some(ThemeMode::Light));
        assert_eq!(options.size, Some((1600, 1000)));
        assert_eq!(options.hold_seconds, Some(12));
        assert!(options.fake_daemon);
    }

    #[test]
    fn refuses_bad_values() {
        assert!(parse(&["--theme", "blue"]).is_err());
        assert!(parse(&["--size", "1280"]).is_err());
        assert!(parse(&["--size", "10x10"]).is_err());
        assert!(parse(&["--hold", "soon"]).is_err());
        assert!(parse(&["--hold"]).is_err());
        assert!(parse(&["--gate2"]).is_err());
        assert!(parse(&["--fast"]).is_err());
        assert!(parse(&["--shot", "tasks"]).is_err());
        assert!(parse(&["--shot"]).is_err());
        assert_eq!(
            parse(&["--open-task"]),
            Err("--open-task needs a value".to_string())
        );
    }

    #[test]
    fn opens_a_task_with_the_fake_daemon_or_with_gate_2() {
        let options = parse(&["--fake-daemon", "--open-task", "T-0001"]).unwrap();
        assert!(options.fake_daemon);
        assert_eq!(options.open_task.as_deref(), Some("T-0001"));
        let options = parse(&["--open-task", "T-0001", "--gate2", "T-0002"]).unwrap();
        assert_eq!(options.open_task.as_deref(), Some("T-0001"));
        assert_eq!(options.gate2.as_deref(), Some("T-0002"));
    }

    #[test]
    fn every_shot_name_parses_back() {
        for shot in Shot::ALL {
            assert_eq!(parse(&["--shot", shot.name()]).unwrap().shot, Some(shot));
        }
    }

    /// CI shoots the states its workflow lists: the same names, in the same order.
    #[test]
    fn the_ci_workflow_shoots_every_state() {
        let workflow = include_str!("../../../../.github/workflows/rust.yml");
        let part = |start: &str, end: &str| -> Vec<&str> {
            let text = workflow
                .split(start)
                .nth(1)
                .and_then(|rest| rest.split(end).next());
            text.unwrap().split_whitespace().collect()
        };
        let names: Vec<&str> = Shot::ALL.iter().map(|shot| shot.name()).collect();
        assert_eq!(part("STATES: >-", "DOCKED:"), names);
        // The docked size: the table, and every state of screens 2 to 5.
        let docked = part("DOCKED: >-", "run:");
        assert_eq!(docked.first(), Some(&"tasks-table"));
        assert_eq!(docked[1..], names[16..]);
    }
}
