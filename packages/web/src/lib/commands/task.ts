import type { EventType } from '@natamux/core'
import { actionsFor } from '../tasks'
import type { Command, CommandContext } from './types'

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

export const taskActionCommands: Command[] = TASK_ACTIONS.map(({ type, title, key }) => ({
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

export const stageCommands: Command[] = [
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
]

export const startCommands: Command[] = [
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
]

export const taskKeyCommands: Command[] = [
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
]

export const rowCommands: Command[] = [
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
]
