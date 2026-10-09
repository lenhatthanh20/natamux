import type { Task } from '@helmsman/core'
import { describe, expect, it, vi } from 'vitest'
import { closeSocket, liveSocketUrl, parseLiveMessage } from './live'

const task: Task = {
  id: 'T-0001',
  title: 'Example',
  description: '',
  state: 'review',
  priority: 'normal',
  labels: [],
  agent: null,
  branch: null,
  workspace: null,
  pr: null,
  prStatus: null,
  parentId: null,
  repo: null,
  dependsOn: [],
  createdAt: '2026-10-08T09:00:00.000Z',
  updatedAt: '2026-10-08T09:00:00.000Z',
}

describe('liveSocketUrl', () => {
  it('uses ws for http and wss for https', () => {
    expect(liveSocketUrl({ protocol: 'http:', host: 'localhost:5173' }, 'abc')).toBe(
      'ws://localhost:5173/api/events?token=abc',
    )
    expect(liveSocketUrl({ protocol: 'https:', host: 'app.github.dev' }, 'abc')).toBe(
      'wss://app.github.dev/api/events?token=abc',
    )
  })

  it('encodes the token', () => {
    expect(liveSocketUrl({ protocol: 'http:', host: 'localhost' }, 'a b&c')).toContain(
      'token=a%20b%26c',
    )
  })
})

describe('parseLiveMessage', () => {
  it('reads the task of a created message', () => {
    expect(parseLiveMessage(JSON.stringify({ kind: 'task.created', task }))).toEqual(task)
  })

  it('reads the task of an updated message and ignores the other fields', () => {
    const text = JSON.stringify({ kind: 'task.updated', task, accepted: true, event: {} })
    expect(parseLiveMessage(text)).toEqual(task)
  })

  it.each([
    ['text that is not JSON', 'not json'],
    ['JSON that is not an object', '42'],
    ['a message without a task', JSON.stringify({ kind: 'task.updated' })],
    ['a task that is not valid', JSON.stringify({ task: { id: 'nope' } })],
  ])('ignores %s', (_name, text) => {
    expect(parseLiveMessage(text)).toBeUndefined()
  })
})

describe('closeSocket', () => {
  function fake(readyState: number) {
    return {
      readyState,
      close: vi.fn(),
      onopen: null as (() => void) | null,
      onmessage: vi.fn() as unknown,
      onclose: vi.fn() as unknown,
      onerror: vi.fn() as unknown,
    }
  }

  it('closes an open socket at once', () => {
    const socket = fake(1)
    closeSocket(socket as unknown as WebSocket)
    expect(socket.close).toHaveBeenCalledTimes(1)
  })

  it('waits for a socket that is still connecting, so the browser logs no warning', () => {
    const socket = fake(0)
    closeSocket(socket as unknown as WebSocket)
    expect(socket.close).not.toHaveBeenCalled()
    expect(socket.onmessage).toBeNull()
    expect(socket.onclose).toBeNull()
    socket.onopen?.()
    expect(socket.close).toHaveBeenCalledTimes(1)
  })
})
