import type { EventType, TaskState } from '@natamux/core'
import { fireAppEvent } from './app-events'
import { actionsFor } from './tasks'
import { applyTheme, type Theme } from './theme'
import { applyUiSize, type UiSize } from './ui-size'
import type { Layout, WorkspaceTab } from './view-store'

// Where the keyboard focus is when a key is pressed. A terminal and a field keep the keys they
// type; the body and the main area are where single-key commands work.
export type FocusKind = 'terminal' | 'field' | 'body' | 'control'

export type CommandState = {
  focus: FocusKind
  // A workspace (terminal, changes, history) fills the main area.
  workspaceOpen: boolean
  agentsOpen: boolean
  // A task is selected in the list, or is the open workspace.
  hasSelection: boolean
  selectedState: TaskState | null
}

// What a command can make the app do. The dashboard implements it.
export type CommandActions = {
  openPalette: (query?: string) => void
  openJump: (scope?: 'needs-you') => void
  toggleSidebar: () => void
  toggleDetails: () => void
  showShortcuts: () => void
  focusFilter: () => void
  newWorkspace: () => void
  newTask: () => void
  moveSelection: (step: number) => void
  openSelected: () => void
  toggleSelectedDetails: () => void
  editSelected: () => void
  taskEvent: (type: EventType) => void
  stageAction: (action: 'skip' | 'approve' | 'report') => void
  showTab: (tab: WorkspaceTab) => void
  // Opens the details of the task on its progress.
  showProgress: () => void
  back: () => void
  showAllTasks: () => void
  showAgents: () => void
  setLayout: (layout: Layout) => void
  openRepos: () => void
  openImport: () => void
}

export type CommandContext = { state: CommandState; actions: CommandActions }

export type CommandCategory = 'App' | 'View' | 'Task' | 'Workspace' | 'Theme' | 'Project'

// The groups of the shortcut sheet, in the order of the keyboard map in the UI spec (5.1).
export const SHEET_GROUPS = [
  'Global',
  'Single keys',
  'Lists and trees',
  'Panes',
  'Diff',
  'Palette',
] as const
export type SheetGroup = (typeof SHEET_GROUPS)[number]

export type Command = {
  id: string
  title: string
  category: CommandCategory
  // For example 'Mod+K'. The first one is shown in tooltips and menus.
  shortcuts?: readonly string[]
  // Where the shortcut sheet lists it. Commands without a group are only in the palette.
  group?: SheetGroup
  // Where its keys work, when that is not the default (everywhere for chords, outside fields
  // and terminals for single keys).
  focus?: readonly FocusKind[]
  when?: (context: CommandContext) => boolean
  // Without run, the entry only documents a key that a control handles itself.
  run?: (context: CommandContext) => void
  // Not in the palette: it would only open itself or move a list.
  hidden?: boolean
  // Shown in the palette before the user types.
  suggested?: boolean
}

const inWorkspace = ({ state }: CommandContext) => state.workspaceOpen
const hasTask = ({ state }: CommandContext) => state.hasSelection

// Task actions that can run from a key or the palette. Cancel asks first, so it is not here.
const TASK_ACTIONS: readonly { type: EventType; title: string; key?: string }[] = [
  { type: 'task.readied', title: 'Mark ready' },
  { type: 'task.blocked', title: 'Block' },
  { type: 'agent.resumed', title: 'Resume the agent' },
  { type: 'review.approved', title: 'Approve', key: 'a' },
  { type: 'review.changes-requested', title: 'Request changes', key: 'r' },
  { type: 'task.unblocked', title: 'Unblock' },
  { type: 'task.retried', title: 'Retry' },
]

const taskActionCommands: Command[] = TASK_ACTIONS.map(({ type, title, key }) => ({
  id: `task.${type}`,
  title,
  category: 'Task',
  shortcuts: key === undefined ? undefined : [key],
  group: key === undefined ? undefined : 'Single keys',
  when: ({ state }) =>
    state.selectedState !== null &&
    actionsFor(state.selectedState).some((action) => action.type === type && !action.confirm),
  run: ({ actions }) => actions.taskEvent(type),
}))

// Stage actions that need no more input. Send back asks for a stage and a reason, so it opens
// the progress in the details, where that form is.
const STAGE_ACTIONS: readonly {
  action: 'skip' | 'approve' | 'report'
  title: string
}[] = [
  { action: 'skip', title: 'Skip the stage' },
  { action: 'report', title: 'Mark the stage as done' },
  { action: 'approve', title: 'Approve the stage anyway' },
]

const inStage = ({ state }: CommandContext) =>
  state.workspaceOpen &&
  state.selectedState !== null &&
  ['ready', 'running', 'needs-input', 'review'].includes(state.selectedState)

const stageCommands: Command[] = [
  ...STAGE_ACTIONS.map(
    ({ action, title }): Command => ({
      id: `stage.${action}`,
      title,
      category: 'Workspace',
      when: inStage,
      run: ({ actions }) => actions.stageAction(action),
    }),
  ),
  {
    id: 'stage.send-back',
    title: 'Send the task back to an earlier stage',
    category: 'Workspace',
    when: inStage,
    run: ({ actions }) => actions.showProgress(),
  },
  {
    id: 'progress.show',
    title: 'Show the task progress',
    category: 'Workspace',
    when: ({ state }) => state.workspaceOpen || state.hasSelection,
    run: ({ actions }) => actions.showProgress(),
  },
]

const THEMES: readonly { theme: Theme; title: string }[] = [
  { theme: 'system', title: 'Follow the system' },
  { theme: 'light', title: 'Light' },
  { theme: 'dark', title: 'Dark' },
]
const UI_SIZES: readonly { size: UiSize; title: string }[] = [
  { size: 'compact', title: 'Compact' },
  { size: 'default', title: 'Default' },
  { size: 'large', title: 'Large' },
]

const TABS: readonly { tab: WorkspaceTab; key: string; title: string }[] = [
  { tab: 'terminal', key: '1', title: 'Show the Terminal tab' },
  { tab: 'changes', key: '2', title: 'Show the Changes tab' },
  { tab: 'history', key: '3', title: 'Show the History tab' },
  { tab: 'docs', key: '4', title: 'Show the Docs tab' },
]

export const COMMANDS: readonly Command[] = [
  // Global
  {
    id: 'palette.open',
    title: 'Open the command palette',
    category: 'App',
    shortcuts: ['Mod+K', 'F1'],
    group: 'Global',
    hidden: true,
    run: ({ actions }) => actions.openPalette(),
  },
  {
    id: 'jump.open',
    title: 'Jump to a workspace',
    category: 'Workspace',
    shortcuts: ['Mod+J'],
    group: 'Global',
    suggested: true,
    run: ({ actions }) => actions.openJump(),
  },
  {
    id: 'sidebar.toggle',
    title: 'Show or hide the sidebar',
    category: 'View',
    shortcuts: ['Mod+B'],
    group: 'Global',
    run: ({ actions }) => actions.toggleSidebar(),
  },
  {
    id: 'details.toggle',
    title: 'Show or hide the details',
    category: 'View',
    shortcuts: ['Mod+Alt+B'],
    group: 'Global',
    run: ({ actions }) => actions.toggleDetails(),
  },
  {
    id: 'focus.next',
    title: 'Move to the next part of the window',
    category: 'View',
    shortcuts: ['F6'],
    group: 'Global',
    hidden: true,
    run: () => fireAppEvent('focus-part', 1),
  },
  {
    id: 'focus.previous',
    title: 'Move to the previous part of the window',
    category: 'View',
    shortcuts: ['Shift+F6'],
    group: 'Global',
    hidden: true,
    run: () => fireAppEvent('focus-part', -1),
  },
  {
    id: 'form.submit',
    title: 'Submit the current form',
    category: 'App',
    shortcuts: ['Mod+Enter'],
    group: 'Global',
    hidden: true,
  },
  {
    id: 'escape',
    title: 'Close the top layer, clear a field, or go back to All tasks',
    category: 'App',
    shortcuts: ['Escape'],
    group: 'Global',
    focus: ['body', 'control', 'field'],
    hidden: true,
    run: ({ actions }) => actions.back(),
  },

  // Single keys
  {
    id: 'help.shortcuts',
    title: 'Show the keyboard shortcuts',
    category: 'App',
    shortcuts: ['?'],
    group: 'Single keys',
    suggested: true,
    run: ({ actions }) => actions.showShortcuts(),
  },
  {
    id: 'filter.focus',
    title: 'Search the sidebar',
    category: 'View',
    shortcuts: ['/'],
    group: 'Single keys',
    run: ({ actions }) => actions.focusFilter(),
  },
  {
    id: 'workspace.new',
    title: 'Start a task',
    category: 'Task',
    shortcuts: ['c'],
    group: 'Single keys',
    suggested: true,
    run: ({ actions }) => actions.newWorkspace(),
  },
  {
    id: 'task.new',
    title: 'Add a task',
    category: 'Task',
    shortcuts: ['n'],
    group: 'Single keys',
    suggested: true,
    run: ({ actions }) => actions.newTask(),
  },
  {
    id: 'list.next',
    title: 'Select the next task',
    category: 'Task',
    shortcuts: ['j', 'ArrowDown'],
    group: 'Single keys',
    hidden: true,
    when: ({ state }) => !state.workspaceOpen && !state.agentsOpen,
    run: ({ actions }) => actions.moveSelection(1),
  },
  {
    id: 'list.previous',
    title: 'Select the previous task',
    category: 'Task',
    shortcuts: ['k', 'ArrowUp'],
    group: 'Single keys',
    hidden: true,
    when: ({ state }) => !state.workspaceOpen && !state.agentsOpen,
    run: ({ actions }) => actions.moveSelection(-1),
  },
  {
    id: 'task.open',
    title: 'Open the selected task',
    category: 'Task',
    shortcuts: ['Enter'],
    group: 'Single keys',
    // A focused button or row keeps its own Enter.
    focus: ['body'],
    hidden: true,
    when: hasTask,
    run: ({ actions }) => actions.openSelected(),
  },
  {
    id: 'task.details',
    title: 'Show or hide the details of the selected task',
    category: 'Task',
    shortcuts: ['i'],
    group: 'Single keys',
    when: hasTask,
    run: ({ actions }) => actions.toggleSelectedDetails(),
  },
  {
    id: 'task.edit',
    title: 'Edit the selected task',
    category: 'Task',
    shortcuts: ['e'],
    group: 'Single keys',
    when: hasTask,
    run: ({ actions }) => actions.editSelected(),
  },
  ...taskActionCommands,
  ...TABS.map(
    ({ tab, key, title }): Command => ({
      id: `tab.${tab}`,
      title,
      category: 'View',
      shortcuts: [key],
      group: 'Single keys',
      when: inWorkspace,
      run: ({ actions }) => actions.showTab(tab),
    }),
  ),
  ...stageCommands,
  {
    id: 'changes.refresh',
    title: 'Read the changes and the history again',
    category: 'Workspace',
    shortcuts: ['Shift+R'],
    group: 'Single keys',
    when: inWorkspace,
    run: () => fireAppEvent('refresh-changes'),
  },

  // Lists and trees: the controls handle these keys themselves.
  {
    id: 'list.ends',
    title: 'First or last row',
    category: 'App',
    shortcuts: ['Home', 'End'],
    group: 'Lists and trees',
    hidden: true,
  },
  {
    id: 'tree.collapse',
    title: 'Collapse or expand a group',
    category: 'App',
    shortcuts: ['ArrowLeft', 'ArrowRight'],
    group: 'Lists and trees',
    hidden: true,
  },
  {
    id: 'row.rename',
    title: 'Rename the workspace',
    category: 'App',
    shortcuts: ['F2'],
    group: 'Lists and trees',
    hidden: true,
  },
  {
    id: 'row.menu',
    title: 'Open the menu of the row',
    category: 'App',
    shortcuts: ['Shift+F10', 'ContextMenu'],
    group: 'Lists and trees',
    hidden: true,
  },
  {
    id: 'row.remove',
    title: 'Remove the workspace (asks first)',
    category: 'App',
    shortcuts: ['Delete'],
    group: 'Lists and trees',
    hidden: true,
  },
  {
    // The stage list of Project settings moves its focused row.
    id: 'stage.move',
    title: 'Move a stage',
    category: 'Project',
    shortcuts: ['Alt+ArrowUp', 'Alt+ArrowDown'],
    group: 'Lists and trees',
    hidden: true,
  },

  // Panes
  {
    id: 'pane.maximize',
    title: 'Maximize or restore the focused pane',
    category: 'Workspace',
    shortcuts: ['Mod+Shift+Enter'],
    group: 'Panes',
    run: () => fireAppEvent('pane-maximize'),
  },
  {
    id: 'shell.new',
    title: 'New shell',
    category: 'Workspace',
    shortcuts: ['Ctrl+Shift+`'],
    group: 'Panes',
    when: inWorkspace,
    run: () => fireAppEvent('new-shell'),
  },
  {
    id: 'terminal.find',
    title: 'Find in the terminal',
    category: 'Workspace',
    shortcuts: ['Mod+F', 'Mod+Shift+F'],
    group: 'Panes',
    hidden: true,
    when: ({ state }) => state.focus === 'terminal',
    run: () => fireAppEvent('terminal-find'),
  },

  // Diff
  {
    id: 'diff.previous',
    title: 'Previous changed file',
    category: 'Workspace',
    shortcuts: ['['],
    group: 'Diff',
    when: inWorkspace,
    run: () => fireAppEvent('diff-step', -1),
  },
  {
    id: 'diff.next',
    title: 'Next changed file',
    category: 'Workspace',
    shortcuts: [']'],
    group: 'Diff',
    when: inWorkspace,
    run: () => fireAppEvent('diff-step', 1),
  },
  {
    id: 'diff.nextChange',
    title: 'Next change in the file',
    category: 'Workspace',
    shortcuts: ['Alt+F5'],
    group: 'Diff',
    when: inWorkspace,
    run: () => fireAppEvent('diff-change-step', 1),
  },
  {
    id: 'diff.previousChange',
    title: 'Previous change in the file',
    category: 'Workspace',
    shortcuts: ['Shift+Alt+F5'],
    group: 'Diff',
    when: inWorkspace,
    run: () => fireAppEvent('diff-change-step', -1),
  },
  {
    // Monaco handles these keys itself, in the diff it has the focus of.
    id: 'diff.accessible',
    title: 'Accessible diff view',
    category: 'Workspace',
    shortcuts: ['F7', 'Shift+F7'],
    group: 'Diff',
    hidden: true,
  },
  {
    id: 'diff.comment',
    title: 'Comment on the file for the agent',
    category: 'Workspace',
    shortcuts: ['Mod+Shift+A'],
    group: 'Diff',
    when: inWorkspace,
    run: () => fireAppEvent('diff-comment'),
  },
  {
    id: 'diff.draftCommit',
    title: 'Draft the commit message with the agent',
    category: 'Workspace',
    shortcuts: ['Mod+Shift+G'],
    group: 'Diff',
    // Only in the commit box (#commit-message of ChangesPanel): elsewhere the key would start an
    // agent run, and its draft could replace text the user did not mean to give up.
    focus: ['field'],
    when: (context) => inWorkspace(context) && document.activeElement?.id === 'commit-message',
    run: () => fireAppEvent('commit-draft'),
  },

  // The palette's own keys
  {
    id: 'palette.move',
    title: 'Move in the list',
    category: 'App',
    shortcuts: ['ArrowUp', 'ArrowDown'],
    group: 'Palette',
    hidden: true,
  },
  {
    id: 'palette.open-row',
    title: 'Open the row, or add it to Agents',
    category: 'App',
    shortcuts: ['Enter', 'Shift+Enter'],
    group: 'Palette',
    hidden: true,
  },
  {
    id: 'palette.digit',
    title: 'Pick row 1 to 9',
    category: 'App',
    shortcuts: ['Alt+1'],
    group: 'Palette',
    hidden: true,
  },
  {
    id: 'palette.scope',
    title: 'Change the scope',
    category: 'App',
    shortcuts: ['Tab'],
    group: 'Palette',
    hidden: true,
  },

  // Only in the palette
  {
    id: 'view.tasks',
    title: 'All tasks',
    category: 'View',
    run: ({ actions }) => actions.showAllTasks(),
  },
  {
    id: 'view.agents',
    title: 'Agents',
    category: 'View',
    suggested: true,
    run: ({ actions }) => actions.showAgents(),
  },
  {
    id: 'view.table',
    title: 'Show tasks as a table',
    category: 'View',
    run: ({ actions }) => actions.setLayout('table'),
  },
  {
    id: 'view.board',
    title: 'Show tasks as a board',
    category: 'View',
    run: ({ actions }) => actions.setLayout('board'),
  },
  {
    id: 'view.progress',
    title: 'Show tasks against their stages',
    category: 'View',
    run: ({ actions }) => actions.setLayout('progress'),
  },
  ...THEMES.map(
    ({ theme, title }): Command => ({
      id: `theme.${theme}`,
      title,
      category: 'Theme',
      run: () => applyTheme(theme),
    }),
  ),
  ...UI_SIZES.map(
    ({ size, title }): Command => ({
      id: `ui-size.${size}`,
      title: `Text size: ${title}`,
      category: 'View',
      run: () => applyUiSize(size),
    }),
  ),
  {
    id: 'repo.add',
    title: 'Add a project',
    category: 'Project',
    run: ({ actions }) => actions.openRepos(),
  },
  {
    id: 'repo.import',
    title: 'Import GitHub issues',
    category: 'Project',
    run: ({ actions }) => actions.openImport(),
  },
]

export function commandById(id: string): Command | undefined {
  return COMMANDS.find((command) => command.id === id)
}

// The first shortcut of a command, for a tooltip or a menu row.
export function shortcutOf(id: string): string | undefined {
  return commandById(id)?.shortcuts?.[0]
}

// The title of a command in the palette: "Task: Approve".
export function paletteTitle(command: Command): string {
  return `${command.category}: ${command.title}`
}

export function sheetGroups(
  commands: readonly Command[] = COMMANDS,
): { group: SheetGroup; commands: Command[] }[] {
  return SHEET_GROUPS.map((group) => ({
    group,
    commands: commands.filter((command) => command.group === group),
  })).filter(({ commands: inGroup }) => inGroup.length > 0)
}

export function runnable(command: Command, context: CommandContext): boolean {
  return command.run !== undefined && (command.when?.(context) ?? true)
}
