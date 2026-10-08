import type { Task } from '@helmsman/core'
import { describe, expect, it } from 'vitest'
import { liveSocketUrl, parseLiveMessage } from './live'

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
