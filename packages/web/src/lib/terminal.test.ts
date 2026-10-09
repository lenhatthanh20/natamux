import { describe, expect, it } from 'vitest'
import { parseTerminalMessage, takesFocusOnOpen, terminalSocketUrl } from './terminal'

describe('terminalSocketUrl', () => {
  it('builds the socket address through /api with the ticket and the size', () => {
    expect(
      terminalSocketUrl({ protocol: 'https:', host: 'example.dev' }, 'T-0001', 'a+b/c', {
        cols: 80,
        rows: 24,
      }),
    ).toBe(
      'wss://example.dev/api/tasks/T-0001/terminal?ticket=a%2Bb%2Fc&cols=80&rows=24&terminal=1',
    )
    expect(
      terminalSocketUrl(
        { protocol: 'http:', host: 'localhost:5173' },
        'T-0002',
        't',
        { cols: 1, rows: 2 },
        '3',
      ),
    ).toBe('ws://localhost:5173/api/tasks/T-0002/terminal?ticket=t&cols=1&rows=2&terminal=3')
  })
})

describe('parseTerminalMessage', () => {
  it.each([
    ['{"type":"output","data":"hi"}', { type: 'output', data: 'hi' }],
    ['{"type":"exit","code":0}', { type: 'exit', code: 0 }],
    ['{"type":"exit","code":null}', { type: 'exit', code: null }],
    ['{"type":"error","message":"no"}', { type: 'error', message: 'no' }],
  ])('reads %s', (text, message) => {
    expect(parseTerminalMessage(text)).toEqual(message)
  })

  it.each(['not json', '{"type":"other"}', '{"type":"output"}', 'null'])('ignores %s', (text) => {
    expect(parseTerminalMessage(text)).toBeUndefined()
  })
})

describe('takesFocusOnOpen', () => {
  it('leaves the focus on the page and on a tab, and takes it from other controls', () => {
    const tab = document.createElement('button')
    tab.setAttribute('role', 'tab')
    const row = document.createElement('button')
    expect(takesFocusOnOpen(document.body, document.body)).toBe(false)
    expect(takesFocusOnOpen(null, document.body)).toBe(false)
    expect(takesFocusOnOpen(tab, document.body)).toBe(false)
    expect(takesFocusOnOpen(row, document.body)).toBe(true)
  })
})
