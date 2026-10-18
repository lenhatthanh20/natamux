import { describe, expect, it } from 'vitest'
import {
  isFunctionKey,
  isSingleKey,
  matchesShortcut,
  parseShortcut,
  reachesAppFromTerminal,
  type ShortcutEvent,
} from '../shortcut'

function press(key: string, modifiers: Partial<ShortcutEvent> = {}): ShortcutEvent {
  return { key, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...modifiers }
}

describe('parseShortcut', () => {
  it('reads modifiers and the key', () => {
    expect(parseShortcut('Mod+Shift+Enter')).toEqual({
      mod: true,
      ctrl: false,
      alt: false,
      shift: true,
      key: 'enter',
    })
    expect(parseShortcut('?').key).toBe('?')
    expect(parseShortcut('Ctrl+Shift+`')).toMatchObject({ ctrl: true, shift: true, key: '`' })
  })
})

describe('matchesShortcut', () => {
  it('maps Mod to Cmd on a Mac and to Ctrl elsewhere', () => {
    expect(matchesShortcut(press('k', { metaKey: true }), 'Mod+K', true)).toBe(true)
    expect(matchesShortcut(press('k', { ctrlKey: true }), 'Mod+K', true)).toBe(false)
    expect(matchesShortcut(press('k', { ctrlKey: true }), 'Mod+K', false)).toBe(true)
    expect(matchesShortcut(press('k', { metaKey: true }), 'Mod+K', false)).toBe(false)
  })

  it('needs every modifier of the shortcut and no other', () => {
    expect(matchesShortcut(press('b', { ctrlKey: true, altKey: true }), 'Mod+Alt+B', false)).toBe(
      true,
    )
    expect(matchesShortcut(press('b', { ctrlKey: true }), 'Mod+Alt+B', false)).toBe(false)
    expect(matchesShortcut(press('b', { ctrlKey: true, altKey: true }), 'Mod+B', false)).toBe(false)
    expect(matchesShortcut(press('c'), 'c', true)).toBe(true)
    expect(matchesShortcut(press('c', { metaKey: true }), 'c', true)).toBe(false)
  })

  it('tells a letter from the same letter with Shift', () => {
    expect(matchesShortcut(press('r'), 'r', false)).toBe(true)
    expect(matchesShortcut(press('R', { shiftKey: true }), 'r', false)).toBe(false)
    expect(matchesShortcut(press('R', { shiftKey: true }), 'Shift+R', false)).toBe(true)
  })

  it('ignores Shift for a character that needs it, like ?', () => {
    expect(matchesShortcut(press('?', { shiftKey: true }), '?', false)).toBe(true)
    expect(matchesShortcut(press('/'), '/', false)).toBe(true)
  })

  it('finds a key by its place when Option or Shift changed the character', () => {
    expect(
      matchesShortcut({ ...press('ª', { altKey: true }), code: 'Digit1' }, 'Alt+1', true),
    ).toBe(true)
    expect(
      matchesShortcut(
        { ...press('~', { ctrlKey: true, shiftKey: true }), code: 'Backquote' },
        'Ctrl+Shift+`',
        true,
      ),
    ).toBe(true)
  })

  it('matches named keys and function keys', () => {
    expect(matchesShortcut(press('F6'), 'F6', false)).toBe(true)
    expect(matchesShortcut(press('F6', { shiftKey: true }), 'F6', false)).toBe(false)
    expect(matchesShortcut(press('F6', { shiftKey: true }), 'Shift+F6', false)).toBe(true)
    expect(matchesShortcut(press('Escape'), 'Escape', false)).toBe(true)
  })
})

describe('kinds of shortcut', () => {
  it('knows function keys and single keys', () => {
    expect(isFunctionKey('F1')).toBe(true)
    expect(isFunctionKey('Shift+F6')).toBe(true)
    expect(isFunctionKey('Mod+K')).toBe(false)
    expect(isSingleKey('c')).toBe(true)
    expect(isSingleKey('?')).toBe(true)
    expect(isSingleKey('Shift+R')).toBe(true)
    expect(isSingleKey('Mod+K')).toBe(false)
    expect(isSingleKey('F1')).toBe(false)
  })
})

describe('reachesAppFromTerminal (UI spec 5.1)', () => {
  it('lets every Cmd chord through on a Mac', () => {
    expect(reachesAppFromTerminal('Mod+K', true)).toBe(true)
    expect(reachesAppFromTerminal('Mod+J', true)).toBe(true)
    expect(reachesAppFromTerminal('Mod+B', true)).toBe(true)
  })

  it('keeps Ctrl+K, Ctrl+J and Ctrl+B for the shell elsewhere', () => {
    expect(reachesAppFromTerminal('Mod+K', false)).toBe(false)
    expect(reachesAppFromTerminal('Mod+J', false)).toBe(false)
    expect(reachesAppFromTerminal('Mod+B', false)).toBe(false)
    expect(reachesAppFromTerminal('Mod+Alt+B', false)).toBe(false)
  })

  it('lets function keys and Ctrl+Shift chords through on both', () => {
    for (const mac of [true, false]) {
      expect(reachesAppFromTerminal('F1', mac)).toBe(true)
      expect(reachesAppFromTerminal('F6', mac)).toBe(true)
      expect(reachesAppFromTerminal('Shift+F6', mac)).toBe(true)
      expect(reachesAppFromTerminal('Ctrl+Shift+`', mac)).toBe(true)
      expect(reachesAppFromTerminal('Mod+Shift+Enter', mac)).toBe(true)
    }
  })

  it('never lets a single key through', () => {
    expect(reachesAppFromTerminal('c', true)).toBe(false)
    expect(reachesAppFromTerminal('?', false)).toBe(false)
    expect(reachesAppFromTerminal('Escape', true)).toBe(false)
  })
})
