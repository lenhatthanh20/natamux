import { describe, expect, it } from 'vitest'
import { nextTerminalName, terminalNames } from './terminal-tabs'

describe('terminalNames', () => {
  it('always has the first terminal, in number order, without repeats', () => {
    expect(terminalNames([], [])).toEqual(['1'])
    expect(terminalNames(['10', '2'], ['2', '3'])).toEqual(['1', '2', '3', '10'])
  })
})

describe('nextTerminalName', () => {
  it('takes the number after the highest', () => {
    expect(nextTerminalName(['1'])).toBe('2')
    expect(nextTerminalName(['1', '10', '3'])).toBe('11')
  })
})
