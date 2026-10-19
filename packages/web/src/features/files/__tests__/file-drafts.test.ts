import { act, renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, it } from 'vitest'
import { draftKey, setFileDraft, useFileDrafts, useWarnOnUnsavedFiles } from '../file-drafts'

beforeEach(() => useFileDrafts.setState({ drafts: {} }))

describe('file drafts', () => {
  it('keeps one draft per task and file, and drops it', () => {
    setFileDraft('T-0001', 'a.ts', { text: 'x', version: 'v1' })
    setFileDraft('T-0002', 'a.ts', { text: 'y', version: 'v1' })
    setFileDraft('T-0001', 'a.ts', { text: 'xx', version: 'v1' })
    expect(useFileDrafts.getState().drafts).toEqual({
      [draftKey('T-0001', 'a.ts')]: { text: 'xx', version: 'v1' },
      [draftKey('T-0002', 'a.ts')]: { text: 'y', version: 'v1' },
    })
    setFileDraft('T-0001', 'a.ts', null)
    expect(Object.keys(useFileDrafts.getState().drafts)).toEqual([draftKey('T-0002', 'a.ts')])
  })

  it('asks before the page closes only while a draft is not saved', () => {
    renderHook(() => useWarnOnUnsavedFiles())
    const close = () => {
      const event = new Event('beforeunload', { cancelable: true })
      window.dispatchEvent(event)
      return event.defaultPrevented
    }
    expect(close()).toBe(false)
    act(() => setFileDraft('T-0001', 'a.ts', { text: 'x', version: 'v1' }))
    expect(close()).toBe(true)
    act(() => setFileDraft('T-0001', 'a.ts', null))
    expect(close()).toBe(false)
  })
})
