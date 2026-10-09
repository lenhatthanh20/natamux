import { describe, expect, it, vi } from 'vitest'
import {
  COMMANDS,
  type CommandActions,
  type CommandContext,
  type CommandState,
  commandById,
  paletteTitle,
  runnable,
  SHEET_GROUPS,
  sheetGroups,
  shortcutOf,
} from './commands'

const STATE: CommandState = {
  focus: 'body',
  workspaceOpen: false,
  agentsOpen: false,
  hasSelection: false,
  selectedState: null,
}

function context(state: Partial<CommandState> = {}): {
  context: CommandContext
  actions: CommandActions
} {
  // One mock for each action, so a test can ask which was called.
  const mocks = new Map<string | symbol, ReturnType<typeof vi.fn>>()
  const actions = new Proxy({} as CommandActions, {
    get: (_, name) => {
      const mock = mocks.get(name) ?? vi.fn()
      mocks.set(name, mock)
      return mock
    },
  })
  return { context: { state: { ...STATE, ...state }, actions }, actions }
}

describe('the command registry', () => {
  it('gives every command its own id', () => {
    const ids = COMMANDS.map((command) => command.id)
    expect(new Set(ids).size).toBe(ids.length)
  })

  it('binds a key to one command only, so a shortcut is defined once', () => {
    const seen = new Map<string, string>()
    for (const command of COMMANDS) {
      if (command.run === undefined) {
        continue
      }
      for (const shortcut of command.shortcuts ?? []) {
        expect(seen.get(shortcut), `${shortcut} of ${command.id}`).toBeUndefined()
        seen.set(shortcut, command.id)
      }
    }
  })

  it('lists every group of the keyboard map in the shortcut sheet, each with keys', () => {
    expect(sheetGroups().map(({ group }) => group)).toEqual([...SHEET_GROUPS])
    for (const command of COMMANDS.filter((item) => item.group !== undefined)) {
      expect(command.shortcuts?.length, command.id).toBeGreaterThan(0)
    }
  })

  it('has the keys of section 5.1 of the UI spec', () => {
    expect(shortcutOf('palette.open')).toBe('Mod+K')
    expect(commandById('palette.open')?.shortcuts).toContain('F1')
    expect(shortcutOf('jump.open')).toBe('Mod+J')
    expect(shortcutOf('sidebar.toggle')).toBe('Mod+B')
    expect(shortcutOf('details.toggle')).toBe('Mod+Alt+B')
    expect(shortcutOf('focus.next')).toBe('F6')
    expect(shortcutOf('focus.previous')).toBe('Shift+F6')
    expect(shortcutOf('help.shortcuts')).toBe('?')
    expect(shortcutOf('filter.focus')).toBe('/')
    expect(shortcutOf('workspace.new')).toBe('c')
    expect(shortcutOf('task.new')).toBe('n')
    expect(shortcutOf('pane.maximize')).toBe('Mod+Shift+Enter')
    expect(shortcutOf('shell.new')).toBe('Ctrl+Shift+`')
    expect(shortcutOf('diff.comment')).toBe('Mod+Shift+A')
    expect(shortcutOf('tab.terminal')).toBe('1')
    expect(shortcutOf('tab.changes')).toBe('2')
    expect(shortcutOf('tab.history')).toBe('3')
  })

  it('steps through the changes of a file with Alt+F5, and documents the accessible diff', () => {
    expect(shortcutOf('diff.nextChange')).toBe('Alt+F5')
    expect(shortcutOf('diff.previousChange')).toBe('Shift+Alt+F5')
    expect(commandById('diff.accessible')?.shortcuts).toEqual(['F7', 'Shift+F7'])
    expect(commandById('diff.accessible')?.run).toBeUndefined()
    for (const id of ['diff.nextChange', 'diff.previousChange']) {
      const command = commandById(id)
      expect(command && runnable(command, context().context), id).toBe(false)
      expect(command && runnable(command, context({ workspaceOpen: true }).context), id).toBe(true)
    }
  })

  it('keeps Cancel task out of the keys and the palette, because it asks first', () => {
    expect(commandById('task.task.cancelled')).toBeUndefined()
  })
})

describe('when a command is available', () => {
  it('offers the workspace tabs, files and shells only with a workspace open', () => {
    for (const id of ['tab.terminal', 'tab.changes', 'diff.next', 'diff.previous', 'shell.new']) {
      const command = commandById(id)
      expect(command && runnable(command, context().context), id).toBe(false)
      expect(command && runnable(command, context({ workspaceOpen: true }).context), id).toBe(true)
    }
  })

  it('moves the selection in the task list only', () => {
    const next = commandById('list.next')
    expect(next && runnable(next, context().context)).toBe(true)
    expect(next && runnable(next, context({ workspaceOpen: true }).context)).toBe(false)
    expect(next && runnable(next, context({ agentsOpen: true }).context)).toBe(false)
  })

  it('offers Approve and Request changes in the review state only', () => {
    const approve = commandById('task.review.approved')
    expect(approve?.shortcuts).toEqual(['a'])
    expect(commandById('task.review.changes-requested')?.shortcuts).toEqual(['r'])
    expect(approve && runnable(approve, context({ selectedState: 'running' }).context)).toBe(false)
    expect(approve && runnable(approve, context({ selectedState: null }).context)).toBe(false)
    expect(approve && runnable(approve, context({ selectedState: 'review' }).context)).toBe(true)
  })

  it('offers the stage actions in an open workspace of a task that is not finished', () => {
    const skip = commandById('stage.skip')
    const open = { workspaceOpen: true, selectedState: 'running' as const }
    expect(skip && runnable(skip, context(open).context)).toBe(true)
    expect(skip && runnable(skip, context({ ...open, selectedState: 'done' }).context)).toBe(false)
    expect(skip && runnable(skip, context({ selectedState: 'running' }).context)).toBe(false)
  })

  it('offers Find in the terminal only while a terminal has the focus', () => {
    const find = commandById('terminal.find')
    expect(find && runnable(find, context({ focus: 'body' }).context)).toBe(false)
    expect(find && runnable(find, context({ focus: 'terminal' }).context)).toBe(true)
  })

  it('does not run entries that only document a key', () => {
    const submit = commandById('form.submit')
    expect(submit?.run).toBeUndefined()
    expect(submit && runnable(submit, context().context)).toBe(false)
  })
})

describe('running a command', () => {
  it('goes through the actions of the app', () => {
    const { context: ctx, actions } = context()
    commandById('workspace.new')?.run?.(ctx)
    expect(actions.newWorkspace).toHaveBeenCalledTimes(1)
    commandById('palette.open')?.run?.(ctx)
    expect(actions.openPalette).toHaveBeenCalledTimes(1)
    commandById('view.board')?.run?.(ctx)
    expect(actions.setLayout).toHaveBeenCalledWith('board')
    commandById('tab.history')?.run?.(ctx)
    expect(actions.showTab).toHaveBeenCalledWith('history')
    commandById('stage.approve')?.run?.(ctx)
    expect(actions.stageAction).toHaveBeenCalledWith('approve')
    commandById('stage.report')?.run?.(ctx)
    expect(actions.stageAction).toHaveBeenCalledWith('report')
    commandById('stage.send-back')?.run?.(ctx)
    expect(actions.showTab).toHaveBeenCalledWith('progress')
    commandById('task.review.approved')?.run?.(ctx)
    expect(actions.taskEvent).toHaveBeenCalledWith('review.approved')
  })

  it('writes the palette title with its category', () => {
    const command = commandById('task.review.approved')
    expect(command && paletteTitle(command)).toBe('Task: Approve')
  })
})
