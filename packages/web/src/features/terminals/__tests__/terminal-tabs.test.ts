import { describe, expect, it } from 'vitest'
import { nextTerminalName, terminalNames, unlistedShells } from '../terminal-tabs'

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

  it('does not take a name that a stage shell or an added shell has', () => {
    // Shell 3 is a stage shell, Shell 4 was added by hand.
    const names = terminalNames(['1', '3'], ['4'])
    expect(nextTerminalName(names)).toBe('5')
    expect(nextTerminalName(['1', '3'])).toBe('4')
  })
})

describe('unlistedShells', () => {
  it('gives the agent shells that the open list does not have', () => {
    expect(unlistedShells(['1'], ['1', '2'])).toEqual(['2'])
    expect(unlistedShells(['1', '2'], ['2'])).toEqual([])
    expect(unlistedShells(['1'], [])).toEqual([])
  })
})
