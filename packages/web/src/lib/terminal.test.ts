import { describe, expect, it } from 'vitest'
import {
  isQueryReply,
  parseTerminalMessage,
  reconnectDelay,
  takesFocusOnOpen,
  terminalSocketUrl,
} from './terminal'

describe('terminalSocketUrl', () => {
  it('builds the socket address through /api with the ticket, the size and the visibility', () => {
    expect(
      terminalSocketUrl({ protocol: 'https:', host: 'example.dev' }, 'T-0001', 'a+b/c', {
        cols: 80,
        rows: 24,
        visible: true,
      }),
    ).toBe(
      'wss://example.dev/api/tasks/T-0001/terminal?ticket=a%2Bb%2Fc&cols=80&rows=24&terminal=1&visible=1',
    )
    expect(
      terminalSocketUrl(
        { protocol: 'http:', host: 'localhost:5173' },
        'T-0002',
        't',
        { cols: 1, rows: 2, visible: false },
        '3',
      ),
    ).toBe(
      'ws://localhost:5173/api/tasks/T-0002/terminal?ticket=t&cols=1&rows=2&terminal=3&visible=0',
    )
  })

  // AC 43: a resume names the shell, and the offset only when the client knows it.
  it('adds the shell id and the offset of a resume', () => {
    const location = { protocol: 'http:', host: 'h' }
    const size = { cols: 80, rows: 24, visible: true }
    expect(terminalSocketUrl(location, 'T-0001', 't', { ...size, shellId: 'a/b' })).toBe(
      'ws://h/api/tasks/T-0001/terminal?ticket=t&cols=80&rows=24&terminal=1&visible=1&shellId=a%2Fb',
    )
    expect(terminalSocketUrl(location, 'T-0001', 't', { ...size, shellId: 's', offset: 0 })).toBe(
      'ws://h/api/tasks/T-0001/terminal?ticket=t&cols=80&rows=24&terminal=1&visible=1&shellId=s&offset=0',
    )
  })
})

// AC 31, C33
describe('reconnectDelay', () => {
  it('doubles from 0.5 s up to 10 s', () => {
    expect([0, 1, 2, 3, 4, 5, 6, 40].map((tries) => reconnectDelay(tries, 0.5))).toEqual([
      500, 1000, 2000, 4000, 8000, 10_000, 10_000, 10_000,
    ])
  })

  it('stays within 20% either way, in whole milliseconds', () => {
    expect(reconnectDelay(0, 0)).toBe(400)
    expect(reconnectDelay(0, 0.999_999_9)).toBe(600)
    expect(reconnectDelay(0, 0.3)).toBe(460)
    expect(reconnectDelay(9, 0)).toBe(8000)
    expect(reconnectDelay(9, 0.999_999_9)).toBe(12_000)
  })
})

describe('parseTerminalMessage', () => {
  it.each([
    ['{"type":"hello","shellId":"abc"}', { type: 'hello', shellId: 'abc' }],
    ['{"type":"snapshot","cols":100,"rows":30}', { type: 'snapshot', cols: 100, rows: 30 }],
    ['{"type":"resume","offset":5}', { type: 'resume', offset: 5 }],
    ['{"type":"replayed","offset":0}', { type: 'replayed', offset: 0 }],
    ['{"type":"pong"}', { type: 'pong' }],
    ['{"type":"exit","code":0}', { type: 'exit', code: 0 }],
    ['{"type":"exit","code":null}', { type: 'exit', code: null }],
    ['{"type":"error","message":"no"}', { type: 'error', message: 'no' }],
  ])('reads %s', (text, message) => {
    expect(parseTerminalMessage(text)).toEqual(message)
  })

  // Output comes in binary frames now, so an output message is unknown (C16).
  it.each([
    'not json',
    '{"type":"other"}',
    '{"type":"output","data":"hi"}',
    '{"type":"output"}',
    '{"type":"snapshot","cols":0,"rows":30}',
    'null',
  ])('ignores %s', (text) => {
    expect(parseTerminalMessage(text)).toBeUndefined()
  })
})

describe('isQueryReply (AC 12)', () => {
  it.each([
    ['DA1', '\x1b[?1;2c'],
    ['DA2', '\x1b[>0;276;0c'],
    ['CPR', '\x1b[12;5R'],
    ['DECXCPR', '\x1b[?12;5R'],
    ['DSR', '\x1b[0n'],
    ['DECRQM', '\x1b[?2026;2$y'],
    ['ANSI DECRQM', '\x1b[4;2$y'],
    ['window size', '\x1b[8;24;80t'],
    ['window size in pixels', '\x1b[4;480;640t'],
    ['DCS', '\x1bP1$r0m\x1b\\'],
    ['OSC with ST', '\x1b]11;rgb:0000/0000/0000\x1b\\'],
    ['OSC with BEL', '\x1b]10;rgb:ffff/ffff/ffff\x07'],
    ['two replies', '\x1b[?1;2c\x1b[12;5R'],
  ])('knows a %s reply', (_name, data) => {
    expect(isQueryReply(data)).toBe(true)
  })

  it.each([
    ['a letter', 'a'],
    ['a Vietnamese letter', 'á'],
    ['DEL', '\x7f'],
    ['text before a reply form', 'abc\x1b[1;2R'],
    ['a mouse report', '\x1b[<0;3;4M'],
    ['a focus report', '\x1b[I'],
    ['nothing', ''],
  ])('does not take %s for a reply', (_name, data) => {
    expect(isQueryReply(data)).toBe(false)
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
