import { afterEach, describe, expect, it, vi } from 'vitest'
import {
  COMMANDS,
  type CommandActions,
  type CommandContext,
  type CommandState,
  type FocusKind,
} from '../commands'
import {
  findCommand,
  focusKind,
  keyBelongsToShell,
  layerIsOpen,
  mayFire,
  publishCommandState,
} from '../dispatch'
import type { ShortcutEvent } from '../shortcut'

function press(key: string, modifiers: Partial<ShortcutEvent> = {}): ShortcutEvent {
  return { key, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...modifiers }
}

const WORKSPACE: CommandState = {
  focus: 'body',
  workspaceOpen: true,
  agentsOpen: false,
  hasSelection: true,
  selectedState: 'review',
}

function inFocus(focus: FocusKind, state: Partial<CommandState> = {}): CommandContext {
  return {
    state: { ...WORKSPACE, ...state, focus },
    actions: new Proxy({} as CommandActions, { get: () => vi.fn() }),
  }
}

function idAt(event: ShortcutEvent, focus: FocusKind, mac: boolean): string | undefined {
  return findCommand(event, inFocus(focus), mac)?.id
}

afterEach(() => {
  document.body.innerHTML = ''
})

describe('the commit message draft key', () => {
  it('does nothing with the focus outside the commit box', () => {
    document.body.innerHTML = `
      <textarea id="commit-message"></textarea>
      <textarea id="agent-comment"></textarea>
      <button id="button">Go</button>`
    const key = press('G', { ctrlKey: true, shiftKey: true })
    for (const id of ['agent-comment', 'button']) {
      const outside = document.getElementById(id)
      outside?.focus()
      expect(findCommand(key, inFocus(focusKind(outside)), false), id).toBeNull()
    }
    document.body.focus()
    expect(findCommand(key, inFocus('body'), false)).toBeNull()

    const box = document.getElementById('commit-message')
    box?.focus()
    expect(findCommand(key, inFocus(focusKind(box)), false)?.id).toBe('diff.draftCommit')
  })
})

describe('focusKind', () => {
  it('tells a terminal, a field, the page and a control apart', () => {
    document.body.innerHTML = `
      <div id="main"></div>
      <div class="xterm"><textarea id="shell"></textarea></div>
      <div class="monaco-editor"><div id="code" tabindex="0"></div></div>
      <input id="field" />
      <select id="select"></select>
      <button id="button">Go</button>`
    const get = (id: string) => document.getElementById(id)
    expect(focusKind(get('shell'))).toBe('terminal')
    expect(focusKind(get('code'))).toBe('field')
    expect(focusKind(get('field'))).toBe('field')
    expect(focusKind(get('select'))).toBe('field')
    expect(focusKind(get('button'))).toBe('control')
    expect(focusKind(get('main'))).toBe('body')
    expect(focusKind(document.body)).toBe('body')
    expect(focusKind(null)).toBe('body')
  })
})

describe('layerIsOpen', () => {
  it('sees a dialog or a menu', () => {
    expect(layerIsOpen()).toBe(false)
    document.body.innerHTML = '<div role="dialog"></div>'
    expect(layerIsOpen()).toBe(true)
  })
})

describe('single keys', () => {
  it('work on the page and on a control', () => {
    expect(idAt(press('c'), 'body', false)).toBe('workspace.new')
    expect(idAt(press('n'), 'control', false)).toBe('task.new')
    expect(idAt(press('?', { shiftKey: true }), 'body', false)).toBe('help.shortcuts')
    expect(idAt(press('2'), 'body', false)).toBe('tab.changes')
    expect(idAt(press('['), 'body', false)).toBe('diff.previous')
    expect(idAt(press('R', { shiftKey: true }), 'body', false)).toBe('changes.refresh')
  })

  it('belong to the field and to the terminal while one has the focus', () => {
    for (const mac of [true, false]) {
      for (const key of ['c', 'n', '/', 'j', 'e', '1']) {
        expect(idAt(press(key), 'field', mac), key).toBeUndefined()
        expect(idAt(press(key), 'terminal', mac), key).toBeUndefined()
      }
    }
  })

  it('keep Enter for a focused control', () => {
    expect(idAt(press('Enter'), 'body', false)).toBe('task.open')
    expect(idAt(press('Enter'), 'control', false)).toBeUndefined()
  })

  it('act on the selected task by its state', () => {
    expect(idAt(press('a'), 'body', false)).toBe('task.review.approved')
    expect(findCommand(press('a'), inFocus('body', { selectedState: 'running' }), false)).toBeNull()
    expect(findCommand(press('e'), inFocus('body', { hasSelection: false }), false)).toBeNull()
  })

  it('do nothing with Cmd or Ctrl held', () => {
    expect(idAt(press('c', { ctrlKey: true }), 'body', false)).toBeUndefined()
    expect(idAt(press('c', { metaKey: true }), 'body', true)).toBeUndefined()
  })
})

describe('chords', () => {
  it('work from a field on both systems', () => {
    expect(idAt(press('k', { metaKey: true }), 'field', true)).toBe('palette.open')
    expect(idAt(press('k', { ctrlKey: true }), 'field', false)).toBe('palette.open')
    expect(idAt(press('b', { ctrlKey: true }), 'field', false)).toBe('sidebar.toggle')
    expect(idAt(press('F1'), 'field', false)).toBe('palette.open')
  })

  it('leave Ctrl+K, Ctrl+J and Ctrl+B to the shell off a Mac', () => {
    expect(idAt(press('k', { ctrlKey: true }), 'terminal', false)).toBeUndefined()
    expect(idAt(press('j', { ctrlKey: true }), 'terminal', false)).toBeUndefined()
    expect(idAt(press('b', { ctrlKey: true }), 'terminal', false)).toBeUndefined()
    expect(idAt(press('b', { ctrlKey: true, altKey: true }), 'terminal', false)).toBeUndefined()
  })

  it('give Cmd+K, Cmd+J and Cmd+B to the app on a Mac, terminal or not', () => {
    expect(idAt(press('k', { metaKey: true }), 'terminal', true)).toBe('palette.open')
    expect(idAt(press('j', { metaKey: true }), 'terminal', true)).toBe('jump.open')
    expect(idAt(press('b', { metaKey: true }), 'terminal', true)).toBe('sidebar.toggle')
    expect(idAt(press('b', { metaKey: true, altKey: true }), 'terminal', true)).toBe(
      'details.toggle',
    )
  })

  it('give F1, F6, Shift+F6 and the Ctrl+Shift chords of the spec to the app in a terminal', () => {
    for (const mac of [true, false]) {
      expect(idAt(press('F1'), 'terminal', mac)).toBe('palette.open')
      expect(idAt(press('F6'), 'terminal', mac)).toBe('focus.next')
      expect(idAt(press('F6', { shiftKey: true }), 'terminal', mac)).toBe('focus.previous')
      const newShell = { ...press('~', { ctrlKey: true, shiftKey: true }), code: 'Backquote' }
      expect(idAt(newShell, 'terminal', mac)).toBe('shell.new')
    }
    const maximizeMac = press('Enter', { metaKey: true, shiftKey: true })
    const maximizeWin = press('Enter', { ctrlKey: true, shiftKey: true })
    expect(idAt(maximizeMac, 'terminal', true)).toBe('pane.maximize')
    expect(idAt(maximizeWin, 'terminal', false)).toBe('pane.maximize')
  })

  it('finds in the terminal only when the terminal has the focus', () => {
    expect(idAt(press('f', { metaKey: true }), 'terminal', true)).toBe('terminal.find')
    expect(idAt(press('f', { metaKey: true }), 'body', true)).toBeUndefined()
    // Ctrl+F is the shell's; Ctrl+Shift+F is the app's.
    expect(idAt(press('f', { ctrlKey: true }), 'terminal', false)).toBeUndefined()
    expect(idAt(press('f', { ctrlKey: true, shiftKey: true }), 'terminal', false)).toBe(
      'terminal.find',
    )
  })

  it('never takes Escape from a terminal', () => {
    expect(idAt(press('Escape'), 'terminal', true)).toBeUndefined()
    expect(idAt(press('Escape'), 'field', true)).toBe('escape')
    expect(idAt(press('Escape'), 'body', true)).toBe('escape')
  })
})

describe('mayFire', () => {
  it('follows the focus list of a command', () => {
    const closer = COMMANDS.find((command) => command.id === 'escape')
    expect(closer && mayFire(closer, 'Escape', 'field', false)).toBe(true)
    expect(closer && mayFire(closer, 'Escape', 'terminal', false)).toBe(false)
  })
})

describe('keyBelongsToShell (what xterm asks)', () => {
  it('says no for the keys of the app and yes for everything else', () => {
    publishCommandState({ ...WORKSPACE, focus: 'terminal' })
    expect(keyBelongsToShell(press('F6'), false)).toBe(false)
    expect(keyBelongsToShell(press('F1'), false)).toBe(false)
    expect(keyBelongsToShell(press('k', { ctrlKey: true }), false)).toBe(true)
    expect(keyBelongsToShell(press('k', { metaKey: true }), true)).toBe(false)
    expect(keyBelongsToShell(press('c'), true)).toBe(true)
    expect(keyBelongsToShell(press('Escape'), true)).toBe(true)
    expect(keyBelongsToShell(press('a', { ctrlKey: true }), false)).toBe(true)
  })

  it('does not take keys of commands that are not available now', () => {
    publishCommandState({ ...WORKSPACE, workspaceOpen: false, focus: 'terminal' })
    const newShell = { ...press('~', { ctrlKey: true, shiftKey: true }), code: 'Backquote' }
    expect(keyBelongsToShell(newShell, false)).toBe(true)
  })
})
