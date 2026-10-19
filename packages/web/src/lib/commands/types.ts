import type { EventType, TaskState } from '@natamux/core'
import type { Layout, WorkspaceTab } from '../view-store/helpers'

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
  openSettings: () => void
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
  // Opens the details of the task on the files of its worktree.
  showFiles: () => void
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
