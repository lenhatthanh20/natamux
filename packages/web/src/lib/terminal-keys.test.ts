import { describe, expect, it } from 'vitest'
import { findShortcut, terminalShortcut } from './terminal-keys'

function key(
  init: Partial<
    Pick<KeyboardEvent, 'key' | 'code' | 'metaKey' | 'ctrlKey' | 'altKey' | 'shiftKey'>
  >,
) {
  return {
    key: '',
    code: '',
    metaKey: false,
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    ...init,
  }
}

describe('terminalShortcut', () => {
  it('takes Cmd chords on a Mac', () => {
    expect(terminalShortcut(key({ key: 'f', metaKey: true }), true)).toBe('find')
    expect(terminalShortcut(key({ key: 'Enter', metaKey: true, shiftKey: true }), true)).toBe(
      'maximize',
    )
  })

  it('leaves Ctrl+F and other Ctrl keys to the shell off a Mac', () => {
    expect(terminalShortcut(key({ key: 'f', ctrlKey: true }), false)).toBeNull()
    expect(terminalShortcut(key({ key: 'k', ctrlKey: true }), false)).toBeNull()
    expect(terminalShortcut(key({ key: 'Enter', ctrlKey: true }), false)).toBeNull()
  })

  it('takes Ctrl+Shift chords off a Mac', () => {
    expect(terminalShortcut(key({ key: 'F', ctrlKey: true, shiftKey: true }), false)).toBe('find')
    expect(terminalShortcut(key({ key: 'Enter', ctrlKey: true, shiftKey: true }), false)).toBe(
      'maximize',
    )
  })

  it('opens a new shell with Ctrl+Shift+` on both systems', () => {
    const event = key({ key: '~', code: 'Backquote', ctrlKey: true, shiftKey: true })
    expect(terminalShortcut(event, true)).toBe('new-shell')
    expect(terminalShortcut(event, false)).toBe('new-shell')
  })

  it('ignores plain keys and Alt chords', () => {
    expect(terminalShortcut(key({ key: 'f' }), true)).toBeNull()
    expect(terminalShortcut(key({ key: 'f', metaKey: true, altKey: true }), true)).toBeNull()
  })
})

describe('findShortcut', () => {
  it('names the key of the platform', () => {
    expect(findShortcut(true)).toBe('Mod+F')
    expect(findShortcut(false)).toBe('Ctrl+Shift+F')
  })
})
