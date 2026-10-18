import { describe, expect, it } from 'vitest'
import type { ChangeEntry } from '../changes.js'
import {
  COMMIT_SUBJECT_MAX,
  commitMessageDraftSchema,
  cutCommitSubject,
  simpleCommitDraft,
} from '../commit-message.js'

function change(
  path: string,
  status: ChangeEntry['status'] = 'M',
  originalPath: string | null = null,
): ChangeEntry {
  return { path, status, originalPath }
}

describe('cutCommitSubject', () => {
  it('keeps a short line', () => {
    expect(cutCommitSubject('Fix the parser')).toBe('Fix the parser')
  })

  it('cuts a long line at the last space before the limit', () => {
    const line = `${'word '.repeat(20)}end`
    const cut = cutCommitSubject(line)
    expect(cut.length).toBeLessThanOrEqual(COMMIT_SUBJECT_MAX)
    expect(line.startsWith(cut)).toBe(true)
    expect(cut.endsWith('word')).toBe(true)
  })

  it('cuts a long line without spaces at the limit', () => {
    expect(cutCommitSubject('a'.repeat(100))).toBe('a'.repeat(COMMIT_SUBJECT_MAX))
  })
})

describe('simpleCommitDraft', () => {
  it.each([
    [change('src/a.ts'), 'Update src/a.ts'],
    [change('src/a.ts', 'A'), 'Add src/a.ts'],
    [change('src/a.ts', 'U'), 'Add src/a.ts'],
    [change('src/a.ts', 'D'), 'Delete src/a.ts'],
    [change('src/b.ts', 'R', 'src/a.ts'), 'Rename src/a.ts to src/b.ts'],
  ])('names the one changed file: %j', (entry, message) => {
    expect(simpleCommitDraft('Fix the parser', [entry])).toBe(message)
  })

  const paths = ['docs/read me.md', 'src/tiếng việt.ts', 'a/b/c.ts']
  it.each(paths)('keeps the path %j as it is, and needs no title', (path) => {
    expect(simpleCommitDraft('', [change(path)])).toBe(`Update ${path}`)
  })

  it('uses the task title and lists the files when more than one changed', () => {
    const draft = simpleCommitDraft('Fix the parser', [
      change('a.ts'),
      change('b.ts', 'U'),
      change('d.ts', 'R', 'c.ts'),
    ])
    expect(draft).toBe('Fix the parser\n\nM a.ts\nU b.ts\nR c.ts -> d.ts')
  })

  it.each([
    [3, 3, null],
    [20, 20, null],
    [21, 20, 'and 1 more files'],
    [50, 20, 'and 30 more files'],
  ])('lists %i files as %i lines', (count, listed, more) => {
    const changes = Array.from({ length: count }, (_, n) => change(`f${n}.ts`))
    const lines = simpleCommitDraft('Fix login', changes).split('\n')
    expect(lines.slice(0, 2)).toEqual(['Fix login', ''])
    expect(lines.slice(2, 2 + listed)).toEqual(changes.slice(0, listed).map((c) => `M ${c.path}`))
    expect(lines.slice(2 + listed)).toEqual(more === null ? [] : [more])
  })

  it('cuts a title over 72 characters and keeps one of exactly 72', () => {
    const two = [change('a.ts'), change('b.ts')]
    const exact = 'x'.repeat(COMMIT_SUBJECT_MAX)
    expect(simpleCommitDraft(exact, two).split('\n')[0]).toBe(exact)
    expect(simpleCommitDraft(`${exact}y`, two).split('\n')[0]).toBe(exact)
  })

  it('returns a string for no changes, and leaves its input alone', () => {
    expect(simpleCommitDraft('Fix login', [])).toBe('Fix login')
    const changes = [change('a.ts'), change('b.ts', 'D')]
    const copy = structuredClone(changes)
    const first = simpleCommitDraft('Fix login', changes)
    expect(simpleCommitDraft('Fix login', changes)).toBe(first)
    expect(changes).toEqual(copy)
  })

  it('keeps the first line within the limit', () => {
    const draft = simpleCommitDraft('x '.repeat(60), [change('a.ts'), change('b.ts')])
    expect(draft.split('\n')[0]?.length).toBeLessThanOrEqual(COMMIT_SUBJECT_MAX)
    const longPath = simpleCommitDraft('Fix', [change(`${'dir/'.repeat(30)}a.ts`)])
    expect(longPath.length).toBeLessThanOrEqual(COMMIT_SUBJECT_MAX)
  })

  it('counts the files when the task has no title', () => {
    expect(simpleCommitDraft('  ', [change('a.ts'), change('b.ts')]).split('\n')[0]).toBe(
      'Update 2 files',
    )
  })
})

describe('commitMessageDraftSchema', () => {
  it('reads a draft with and without a reason', () => {
    const agent = { message: 'Fix it', source: 'agent', agent: 'codex' }
    expect(commitMessageDraftSchema.parse(agent)).toEqual(agent)
    const draft = { message: 'Fix it', source: 'draft', agent: 'codex', reason: 'codex timed out' }
    expect(commitMessageDraftSchema.parse(draft)).toEqual(draft)
    expect(commitMessageDraftSchema.safeParse({ ...agent, source: 'other' }).success).toBe(false)
  })
})
