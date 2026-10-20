import { fireAppEvent } from '../app-events'
import type { WorkspaceTab } from '../view-store/helpers'
import type { Command, CommandContext } from './types'

const inWorkspace = ({ state }: CommandContext) => state.workspaceOpen

const TABS: readonly { tab: WorkspaceTab; key: string; title: string }[] = [
  { tab: 'terminal', key: '1', title: 'Show the Terminal tab' },
  { tab: 'changes', key: '2', title: 'Show the Changes tab' },
  { tab: 'history', key: '3', title: 'Show the History tab' },
  { tab: 'docs', key: '4', title: 'Show the Docs tab' },
]

export const tabCommands: Command[] = TABS.map(
  ({ tab, key, title }): Command => ({
    id: `tab.${tab}`,
    title,
    category: 'View',
    shortcuts: [key],
    group: 'Single keys',
    when: inWorkspace,
    run: ({ actions }) => actions.showTab(tab),
  }),
)

export const workspaceViewCommands: Command[] = [
  {
    id: 'progress.show',
    title: 'Show the task progress',
    category: 'Workspace',
    when: ({ state }) => state.workspaceOpen || state.hasSelection,
    run: ({ actions }) => actions.showProgress(),
  },
  // Palette only: 5 showed the Docs tab before. The workspace ignores it while no file is open.
  {
    id: 'tab.editor',
    title: 'Show the open file',
    category: 'View',
    when: inWorkspace,
    run: ({ actions }) => actions.showTab('editor'),
  },
  {
    id: 'files.show',
    title: 'Show the files of the worktree',
    category: 'Workspace',
    when: inWorkspace,
    run: ({ actions }) => actions.showFiles(),
  },
  {
    id: 'changes.refresh',
    title: 'Read the changes and the history again',
    category: 'Workspace',
    shortcuts: ['Shift+R'],
    group: 'Single keys',
    when: inWorkspace,
    run: () => fireAppEvent('refresh-changes'),
  },
]

export const paneCommands: Command[] = [
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
    title: 'New terminal',
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
]

export const diffCommands: Command[] = [
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
    title: 'Comment on the file or the selection for the agent',
    category: 'Workspace',
    shortcuts: ['Mod+Shift+A'],
    group: 'Diff',
    when: inWorkspace,
    run: () => fireAppEvent('diff-comment'),
  },
  {
    id: 'changes.rollback',
    title: 'Roll back the shown file',
    category: 'Workspace',
    shortcuts: ['Mod+Alt+Z'],
    group: 'Diff',
    when: inWorkspace,
    run: () => fireAppEvent('rollback-file'),
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
]

export const viewCommands: Command[] = [
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
]
