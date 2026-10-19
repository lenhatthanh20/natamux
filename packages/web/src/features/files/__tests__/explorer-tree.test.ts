import type { ChangeEntry, TreeEntry } from '@natamux/core'
import { describe, expect, it } from 'vitest'
import { changeMarks, dropText, explorerRows, pathDragType } from '../explorer-tree'

const folders = new Map<string, TreeEntry[]>([
  [
    '',
    [
      { name: 'src', kind: 'folder' },
      { name: 'docs', kind: 'folder' },
      { name: 'README.md', kind: 'file' },
    ],
  ],
  [
    'src',
    [
      { name: 'lib', kind: 'folder' },
      { name: 'a.ts', kind: 'file' },
    ],
  ],
  ['src/lib', [{ name: 'b.ts', kind: 'file' }]],
])

describe('explorerRows', () => {
  it('shows the root, and the children of the open folders that are loaded', () => {
    const rows = explorerRows(folders, new Set(['src', 'docs']))
    expect(rows.map((row) => [row.key, row.level, row.expanded])).toEqual([
      ['src', 1, true],
      ['src/lib', 2, false],
      ['src/a.ts', 2, false],
      // Open, but not read yet: no children.
      ['docs', 1, true],
      ['README.md', 1, false],
    ])
  })

  it('gives each row its parent and place for the tree keys and screen readers', () => {
    const rows = explorerRows(folders, new Set(['src', 'src/lib']))
    expect(rows.find((row) => row.key === 'src/lib/b.ts')).toMatchObject({
      name: 'b.ts',
      kind: 'file',
      parent: 'src/lib',
      level: 3,
      posInSet: 1,
      setSize: 1,
      expandable: false,
    })
    expect(rows.find((row) => row.key === 'README.md')).toMatchObject({
      parent: null,
      posInSet: 3,
      setSize: 3,
    })
    expect(rows.find((row) => row.key === 'src')).toMatchObject({ expandable: true })
  })

  it('is empty until the root is read', () => {
    expect(explorerRows(new Map(), new Set())).toEqual([])
  })
})

describe('changeMarks', () => {
  it('marks changed files with their status and every folder above them', () => {
    const changes: ChangeEntry[] = [
      { path: 'src/lib/b.ts', status: 'M', originalPath: null },
      { path: 'new.ts', status: 'U', originalPath: null },
    ]
    const marks = changeMarks(changes)
    expect(marks.files.get('src/lib/b.ts')).toBe('M')
    expect(marks.files.get('new.ts')).toBe('U')
    expect([...marks.folders].sort()).toEqual(['src', 'src/lib'])
  })
})

describe('dropText', () => {
  it('is the path and a space, a folder with a slash', () => {
    expect(dropText('packages/web/a.ts', 'file')).toBe('packages/web/a.ts ')
    expect(dropText('packages/web', 'folder')).toBe('packages/web/ ')
    expect(dropText('.github/@types/a+b-c_d.ts', 'file')).toBe('.github/@types/a+b-c_d.ts ')
  })

  it('quotes a path that a shell would split or expand', () => {
    expect(dropText('my docs/a.md', 'file')).toBe("'my docs/a.md' ")
    expect(dropText('it$s.md', 'file')).toBe("'it$s.md' ")
    expect(dropText("it's.md", 'file')).toBe("'it'\\''s.md' ")
  })
})

describe('pathDragType', () => {
  it('names the task in the type, in lower case, so a drop target can check it before the drop', () => {
    expect(pathDragType('T-0118')).toBe('application/x-natamux-path-t-0118')
  })
})
