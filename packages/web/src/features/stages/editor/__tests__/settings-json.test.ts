import { DEFAULT_PIPELINE, type Step } from '@natamux/core'
import { describe, expect, it } from 'vitest'
import {
  mergeChanges,
  parseDraft,
  sameSettings,
  trackerOf,
  withCreateIssue,
  withSteps,
} from '../pipeline-editor/settings-json'

describe('parseDraft', () => {
  it('reads the default pipeline when the file has no stages', () => {
    const draft = parseDraft('{ "tracker": { "type": "none" } }')
    expect(draft.ok && draft.steps).toEqual(DEFAULT_PIPELINE)
  })
  it('refuses text that is not JSON, or not an object', () => {
    const broken = parseDraft('{ "stages": [')
    expect(broken.ok).toBe(false)
    expect(!broken.ok && broken.error).toMatch(/^This is not JSON/)
    expect(parseDraft('[]')).toEqual({
      ok: false,
      error: 'The settings are one JSON object, such as {}.',
    })
  })
  it('refuses stages the list cannot draw', () => {
    const draft = parseDraft('{ "stages": [{ "id": "spec", "kind": "robot", "gate": "auto" }] }')
    expect(!draft.ok && draft.error).toMatch(/^stages, item 1: each stage needs/)
  })
  it('refuses settings outside the stages that break the rules', () => {
    const draft = parseDraft('{ "tracker": { "type": "jira" } }')
    expect(!draft.ok && draft.error).toMatch(/^tracker\.type/)
  })
  it('keeps stages with a wrong field value, for the list to name the problem', () => {
    const draft = parseDraft(
      '{ "stages": [{ "id": "spec", "kind": "agent", "gate": "auto", "output": "x" }] }',
    )
    expect(draft.ok).toBe(true)
  })
})

describe('withSteps', () => {
  it('leaves the stages out when they equal the default, and keeps every other key', () => {
    const raw = { team: 'web', stages: [], tracker: { type: 'none' } }
    expect(withSteps(raw, DEFAULT_PIPELINE)).toEqual({ team: 'web', tracker: { type: 'none' } })
  })
  it('keeps the place of the stages among the keys, or puts them first', () => {
    const steps: Step[] = [{ id: 'implement', kind: 'agent', gate: 'auto' }]
    expect(Object.keys(withSteps({ a: 1, stages: [], b: 2 }, steps))).toEqual(['a', 'stages', 'b'])
    expect(Object.keys(withSteps({ a: 1 }, steps))).toEqual(['stages', 'a'])
  })
})

describe('trackerOf', () => {
  it('reads the tracker of the draft, with the defaults for what it leaves out', () => {
    expect(trackerOf({})).toEqual({ type: 'github', createIssue: 'manual', linkInPr: true })
    expect(trackerOf({ tracker: { type: 'none', createIssue: 'on-pr' } })).toEqual({
      type: 'none',
      createIssue: 'on-pr',
      linkInPr: true,
    })
  })
  it('falls back to the defaults for a tracker it cannot read', () => {
    expect(trackerOf({ tracker: { createIssue: 'always' } }).createIssue).toBe('manual')
    expect(trackerOf({ tracker: 'github' }).type).toBe('github')
  })
})

describe('withCreateIssue', () => {
  it('sets when to open an issue, and keeps every other key', () => {
    const raw = { team: 'web', tracker: { type: 'github', linkInPr: false }, stages: [] }
    const next = withCreateIssue(raw, 'on-spec-approved')
    expect(next).toEqual({
      team: 'web',
      tracker: { type: 'github', linkInPr: false, createIssue: 'on-spec-approved' },
      stages: [],
    })
    expect(Object.keys(next)).toEqual(['team', 'tracker', 'stages'])
    expect(withCreateIssue({ team: 'web' }, 'on-pr')).toEqual({
      team: 'web',
      tracker: { createIssue: 'on-pr' },
    })
  })
  it('keeps keys of the tracker it does not know', () => {
    expect(withCreateIssue({ tracker: { foo: 1 } }, 'never')).toEqual({
      tracker: { foo: 1, createIssue: 'never' },
    })
    expect(withCreateIssue({ tracker: { foo: 1, createIssue: 'never' } }, 'manual')).toEqual({
      tracker: { foo: 1 },
    })
  })
  it('leaves the default out, and a tracker with nothing left', () => {
    const none = { tracker: { type: 'none', createIssue: 'on-pr' } }
    expect(withCreateIssue(none, 'manual')).toEqual({ tracker: { type: 'none' } })
    const only = { team: 'web', tracker: { createIssue: 'on-pr' } }
    expect(withCreateIssue(only, 'manual')).toEqual({ team: 'web' })
  })
})

describe('sameSettings', () => {
  it('compares the settings, not the text', () => {
    expect(sameSettings('{"a":1,"b":[1,2]}', '{\n  "b": [1, 2],\n  "a": 1\n}')).toBe(true)
    expect(sameSettings('{"a":1}', '{"a":2}')).toBe(false)
    expect(sameSettings('{"a":', '{"a":')).toBe(true)
    expect(sameSettings('{"a":', '{"a":1}')).toBe(false)
  })
})

// Cases of .natamux-task/test-spec.md, one block per case.
describe('test spec cases', () => {
  it('C15: keeps the keys of the new file that the person did not change', () => {
    const start = { tracker: { type: 'github' }, agents: { permissionMode: 'auto' } }
    const draft = { ...start, stages: [{ id: 'implement', kind: 'agent', gate: 'auto' }] }
    const now = { tracker: { type: 'none' }, agents: { permissionMode: 'auto' }, team: 'web' }
    expect(mergeChanges(start, draft, now)).toEqual({
      tracker: { type: 'none' },
      agents: { permissionMode: 'auto' },
      team: 'web',
      stages: draft.stages,
    })
  })
  it('C15: removes a key the person removed, and takes the value of the draft on a clash', () => {
    const start = { stages: [], tracker: { type: 'github' } }
    const draft = { tracker: { type: 'none' } }
    const now = { stages: [{ id: 'spec' }], tracker: { type: 'github', linkInPr: false } }
    expect(mergeChanges(start, draft, now)).toEqual({ tracker: { type: 'none' } })
  })
})
