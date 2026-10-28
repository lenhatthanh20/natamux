//! Command line flags. CI uses them to check the app and take screenshots.

use crate::bench::bench_view::{MAX_TERMINALS, Output, TERMINALS};

pub const USAGE: &str =
    "usage: natamux-app [--smoke] [--theme light|dark] [--size WxH] [--hold SECONDS] [--fake-daemon]
                   [--gate2 TASK [--terminals N] [--output flood|agent]]
                   [--shot STATE] [--count-draws] [--open-task TASK]
                   [--bench-flows [--bench-tasks N]]
  --smoke   draw a frame, type into the text box, check it, then quit (exit 1 on failure)
            with --fake-daemon, also check that a workspace's terminal grid does not scroll
            and that the pipeline chart scrolls to its last step
  --theme   use this theme instead of following the system
  --size    window size in points, placed at the top left of the screen
  --hold    quit after this many seconds (for screenshots)
  --fake-daemon  show fixed tasks and a fixed terminal instead of the daemon's (for screenshots)
  --gate2   run gate 2 on TASK's shells 901-960 and 990, save the perf reports, print their paths, then quit
  --terminals  with --gate2: how many shells to bench, 1 to 60 (default 10)
  --output     with --gate2: what they print, flood (1 MB/s of text, the default) or agent (a TUI)
  --shot    open the window in one screen-1 or screen-2 state, for the CI screenshots (docs/rust-phase3.md)
  --count-draws  print the window's draws once a second, to measure the cost of motion
  --open-task    open TASK once the dashboard has it, as a click on its row does
  --bench-flows  time the user flows on the fake daemon 5 times each, save the perf report,
                 print a `flow_bench: {...}` line of numbers, then quit (docs/perf.md)
  --bench-tasks  with --bench-flows: the fake's task count for the event flows (default 200)

The daemon: NATAMUX_URL and NATAMUX_TOKEN, else http://127.0.0.1:4317 and the Natamux app's token.";

/// `--bench-tasks`: the default is the report's "200 tasks, 30 agents" case.
const BENCH_TASKS: usize = 200;
const MAX_BENCH_TASKS: usize = 10_000;

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
    WorkspaceChangesComments,
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
    StageEvidence,
    StartTaskBranchMenu,
    SidebarBlurred,
    RemoveWorkspaceAnyway,
    UpdateDownloading,
    UpdateReady,
    UpdateFailed,
    SettingsUpdateReady,
}

impl Shot {
    pub const ALL: [Shot; 87] = [
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
        Shot::WorkspaceChangesComments,
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
        Shot::StageEvidence,
        Shot::StartTaskBranchMenu,
        Shot::SidebarBlurred,
        Shot::RemoveWorkspaceAnyway,
        Shot::UpdateDownloading,
        Shot::UpdateReady,
        Shot::UpdateFailed,
        Shot::SettingsUpdateReady,
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
            Shot::WorkspaceChangesComments => "workspace-changes-comments",
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
            Shot::StageEvidence => "stage-evidence",
            Shot::StartTaskBranchMenu => "start-task-branch-menu",
            Shot::SidebarBlurred => "sidebar-blurred",
            Shot::RemoveWorkspaceAnyway => "remove-workspace-anyway",
            Shot::UpdateDownloading => "update-downloading",
            Shot::UpdateReady => "update-ready",
            Shot::UpdateFailed => "update-failed",
            Shot::SettingsUpdateReady => "settings-update-ready",
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
    pub gate2_terminals: u16,
    pub gate2_output: Output,
    pub shot: Option<Shot>,
    pub count_draws: bool,
    pub open_task: Option<String>,
    pub bench_flows: bool,
    pub bench_tasks: usize,
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
            gate2_terminals: TERMINALS,
            gate2_output: Output::Flood,
            shot: None,
            count_draws: false,
            open_task: None,
            bench_flows: false,
            bench_tasks: BENCH_TASKS,
        };
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--smoke" => options.smoke = true,
                "--fake-daemon" => options.fake_daemon = true,
                "--count-draws" => options.count_draws = true,
                // The flows run on the fake's data only.
                "--bench-flows" => {
                    options.bench_flows = true;
                    options.fake_daemon = true;
                }
                "--bench-tasks" => options.bench_tasks = parse_tasks(&value(&mut args, &arg)?)?,
                "--theme" => options.theme = Some(parse_theme(&value(&mut args, &arg)?)?),
                "--size" => options.size = Some(parse_size(&value(&mut args, &arg)?)?),
                "--gate2" => options.gate2 = Some(value(&mut args, &arg)?),
                "--terminals" => {
                    options.gate2_terminals = parse_terminals(&value(&mut args, &arg)?)?;
                }
                "--output" => options.gate2_output = parse_output(&value(&mut args, &arg)?)?,
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

fn parse_terminals(text: &str) -> Result<u16, String> {
    match text.parse::<u16>() {
        Ok(count) if (1..=MAX_TERMINALS).contains(&count) => Ok(count),
        _ => Err(format!(
            "--terminals is a number from 1 to {MAX_TERMINALS}, not {text:?}"
        )),
    }
}

fn parse_tasks(text: &str) -> Result<usize, String> {
    match text.parse::<usize>() {
        Ok(count) if (1..=MAX_BENCH_TASKS).contains(&count) => Ok(count),
        _ => Err(format!(
            "--bench-tasks is a number from 1 to {MAX_BENCH_TASKS}, not {text:?}"
        )),
    }
}

fn parse_output(text: &str) -> Result<Output, String> {
    Output::parse(text).ok_or_else(|| format!("--output is flood or agent, not {text:?}"))
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
#[path = "options_tests.rs"]
mod tests;
