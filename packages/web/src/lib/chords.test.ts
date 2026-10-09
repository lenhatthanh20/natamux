import { describe, expect, it } from 'vitest'
import { chordReachesApp, isModChord } from './chords'

const press = (init: Partial<KeyboardEvent> & { key: string }) => ({
  metaKey: false,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  ...init,
})

describe('isModChord', () => {
  it('uses Cmd on a Mac and Ctrl elsewhere', () => {
    expect(isModChord(press({ key: 'b', metaKey: true }), 'b', { mac: true })).toBe(true)
    expect(isModChord(press({ key: 'b', ctrlKey: true }), 'b', { mac: true })).toBe(false)
    expect(isModChord(press({ key: 'b', ctrlKey: true }), 'b', { mac: false })).toBe(true)
  })

  it('tells Mod+B from Mod+Alt+B', () => {
    const plain = press({ key: 'b', ctrlKey: true })
    const alt = press({ key: 'b', ctrlKey: true, altKey: true })
    expect(isModChord(plain, 'b', { mac: false })).toBe(true)
    expect(isModChord(alt, 'b', { mac: false })).toBe(false)
    expect(isModChord(alt, 'b', { alt: true, mac: false })).toBe(true)
  })
})

describe('chordReachesApp', () => {
  it('lets every chord through outside a terminal', () => {
    expect(chordReachesApp(false, false)).toBe(true)
  })

  it('keeps Ctrl chords in the terminal off a Mac', () => {
    expect(chordReachesApp(true, false)).toBe(false)
    expect(chordReachesApp(true, true)).toBe(true)
  })
})
