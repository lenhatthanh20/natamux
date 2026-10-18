import { describe, expect, it } from 'vitest'
import { commandsUnder, descendants, stillRunning } from '../process-tree.js'

describe('descendants', () => {
  it('finds the processes under a shell, at any depth, and no others', () => {
    const ps = [
      '  1     0',
      ' 10     1',
      ' 11    10',
      ' 12    11',
      ' 13    10',
      ' 20     1',
      '',
    ].join('\n')
    expect(descendants(ps, 10)).toEqual([11, 12, 13])
    expect(descendants(ps, 12)).toEqual([])
  })
})

describe('commandsUnder', () => {
  it('gives the command lines of the processes under a shell, parents first', () => {
    const ps = [
      '  1     0 /sbin/launchd',
      ' 10     1 -zsh',
      ' 11    10 node /usr/local/bin/codex --model o3',
      ' 12    11 /usr/local/lib/node_modules/@openai/codex/vendor/codex/codex',
      ' 20     1 vim notes.md',
      '',
    ].join('\n')
    expect(commandsUnder(ps, 10)).toEqual([
      'node /usr/local/bin/codex --model o3',
      '/usr/local/lib/node_modules/@openai/codex/vendor/codex/codex',
    ])
    expect(commandsUnder(ps, 12)).toEqual([])
  })
})

describe('stillRunning', () => {
  it('keeps the processes that still run; a zombie or a missing one has ended', () => {
    const ps = ['  1     0 Ss', ' 11     1 S+', ' 13     1 Z', ''].join('\n')
    expect(stillRunning(ps, [11, 12, 13])).toEqual([11])
    expect(stillRunning(ps, [13])).toEqual([])
    expect(stillRunning(ps, [])).toEqual([])
  })
})
