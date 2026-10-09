import type { ChangeEntry, ChangeStatus } from '@helmsman/core'
import { describe, expect, it } from 'vitest'
import {
  allFolderKeys,
  ancestorKeys,
  buildChangeTree,
  fileOrder,
  flattenTree,
  listRows,
  type TreeFolder,
  type TreeNode,
  totalCounts,
} from './change-tree'

type Entry = ChangeEntry & { added?: number | null; removed?: number | null }

function entry(path: string, status: ChangeStatus = 'M', extra: Partial<Entry> = {}): Entry {
  return { path, status, originalPath: null, ...extra }
}

function folder(node: TreeNode | undefined): TreeFolder {
  if (node?.kind !== 'folder') {
    throw new Error('Expected a folder.')
  }
  return node
}

const names = (nodes: readonly TreeNode[]) => nodes.map((node) => node.name)

describe('buildChangeTree', () => {
  it('puts a file at the top as one file node', () => {
    const tree = buildChangeTree([entry('a.ts')])
    expect(tree).toHaveLength(1)
    expect(tree[0]).toMatchObject({ kind: 'file', key: 'a.ts', name: 'a.ts', depth: 0 })
  })

  it('joins a chain of single folders into one row', () => {
    const tree = buildChangeTree([entry('packages/web/src/a.ts'), entry('packages/web/src/b.ts')])
    expect(tree).toHaveLength(1)
    const src = folder(tree[0])
    expect(src).toMatchObject({ key: 'packages/web/src', name: 'packages/web/src', depth: 0 })
    expect(names(src.children)).toEqual(['a.ts', 'b.ts'])
    expect(src.children[0]?.depth).toBe(1)
  })

  it('stops the chain at a folder that holds a file', () => {
    const tree = buildChangeTree([entry('src/x.ts'), entry('src/a/b/y.ts')])
    const src = folder(tree[0])
    expect(src).toMatchObject({ key: 'src', name: 'src' })
    expect(names(src.children)).toEqual(['a/b', 'x.ts'])
    const ab = folder(src.children[0])
    expect(ab).toMatchObject({ key: 'src/a/b', depth: 1 })
    expect(names(ab.children)).toEqual(['y.ts'])
  })

  it('gives one row per folder without compaction', () => {
    const tree = buildChangeTree([entry('a/b/c.ts')], { compact: false })
    const a = folder(tree[0])
    const b = folder(a.children[0])
    expect([a.key, a.name, b.key, b.name]).toEqual(['a', 'a', 'a/b', 'b'])
    expect(b.children[0]).toMatchObject({ kind: 'file', key: 'a/b/c.ts', depth: 2 })
  })

  it('puts folders first, then sorts naturally and without case', () => {
    expect(names(buildChangeTree([entry('z.ts'), entry('lib/a.ts')]))).toEqual(['lib', 'z.ts'])
    expect(names(buildChangeTree([entry('file10.ts'), entry('file2.ts')]))).toEqual([
      'file2.ts',
      'file10.ts',
    ])
    expect(names(buildChangeTree([entry('b.ts'), entry('A.ts')]))).toEqual(['A.ts', 'b.ts'])
    const one = names(buildChangeTree([entry('a.ts'), entry('A.ts')]))
    const other = names(buildChangeTree([entry('A.ts'), entry('a.ts')]))
    expect(one).toEqual(other)
  })

  it('gives a folder the strongest status below it', () => {
    const statusOf = (...statuses: ChangeStatus[]) => {
      const tree = buildChangeTree(statuses.map((status, index) => entry(`d/${index}.ts`, status)))
      return folder(tree[0]).status
    }
    expect(statusOf('M', 'C')).toBe('C')
    expect(statusOf('D', 'M')).toBe('D')
    expect(statusOf('U', 'A')).toBe('A')
  })

  it('counts the files and sums the lines below a folder', () => {
    const tree = buildChangeTree([
      entry('src/a.ts', 'M', { added: 3, removed: 1 }),
      entry('src/lib/b.ts', 'M', { added: 2, removed: null }),
      entry('src/c.png', 'M', { added: null, removed: null }),
      entry('src/d.ts'),
    ])
    expect(folder(tree[0])).toMatchObject({ fileCount: 4, added: 5, removed: 1 })

    const unknown = buildChangeTree([
      entry('src/a.ts'),
      entry('src/b.png', 'M', { added: null, removed: null }),
    ])
    expect(folder(unknown[0])).toMatchObject({ fileCount: 2, added: null, removed: null })
  })

  it('places a renamed file at its new path', () => {
    const tree = buildChangeTree([entry('src/new.ts', 'R', { originalPath: 'old/old.ts' })])
    expect(names(tree)).toEqual(['src'])
    expect(folder(tree[0]).children[0]).toMatchObject({ kind: 'file', key: 'src/new.ts' })
  })
})

describe('flattenTree', () => {
  const tree = buildChangeTree([entry('src/a.ts'), entry('src/lib/b.ts'), entry('README.md')])

  it('lists the rows with their place in the tree', () => {
    const rows = flattenTree(tree, new Set(['d:src/lib']))
    expect(rows.map((row) => row.key)).toEqual(['d:src', 'd:src/lib', 'f:src/a.ts', 'f:README.md'])
    expect(rows[0]).toMatchObject({
      parent: null,
      level: 1,
      posInSet: 1,
      setSize: 2,
      expandable: true,
      expanded: true,
    })
    expect(rows[1]).toMatchObject({
      parent: 'd:src',
      level: 2,
      posInSet: 1,
      setSize: 2,
      expandable: true,
      expanded: false,
    })
    expect(rows[2]).toMatchObject({
      parent: 'd:src',
      level: 2,
      posInSet: 2,
      setSize: 2,
      expandable: false,
      expanded: false,
    })
    expect(rows[3]).toMatchObject({ parent: null, level: 1, posInSet: 2, setSize: 2 })
  })

  it('leaves out everything below a collapsed folder', () => {
    const rows = flattenTree(tree, new Set(['d:src']))
    expect(rows.map((row) => row.key)).toEqual(['d:src', 'f:README.md'])
  })

  it('gives every folder key', () => {
    expect(allFolderKeys(tree)).toEqual(['d:src', 'd:src/lib'])
  })
})

describe('listRows', () => {
  it('keeps the order given, all at the first level', () => {
    const rows = listRows([entry('b/z.ts'), entry('a.ts')])
    expect(rows.map((row) => [row.key, row.level, row.posInSet, row.setSize])).toEqual([
      ['f:b/z.ts', 1, 1, 2],
      ['f:a.ts', 1, 2, 2],
    ])
    expect(rows[0]?.node.name).toBe('z.ts')
  })
})

describe('fileOrder', () => {
  const list = [entry('z.ts'), entry('src/b.ts'), entry('src/lib/a.ts'), entry('a.ts')]

  it('goes depth first, folders first, in tree mode', () => {
    expect(fileOrder(list, 'tree')).toEqual(['src/lib/a.ts', 'src/b.ts', 'a.ts', 'z.ts'])
  })

  it('keeps the order given in list mode', () => {
    expect(fileOrder(list, 'list')).toEqual(['z.ts', 'src/b.ts', 'src/lib/a.ts', 'a.ts'])
  })
})

describe('ancestorKeys', () => {
  it('gives the compacted folders around a path, outermost first', () => {
    const tree = buildChangeTree([
      entry('packages/web/src/a.ts'),
      entry('packages/web/src/lib/b.ts'),
      entry('packages/web/src/lib/c.ts'),
    ])
    expect(ancestorKeys(tree, 'packages/web/src/lib/b.ts')).toEqual([
      'd:packages/web/src',
      'd:packages/web/src/lib',
    ])
    expect(ancestorKeys(tree, 'other.ts')).toEqual([])
  })
})

describe('totalCounts', () => {
  it('sums the entries that have counts', () => {
    expect(
      totalCounts([
        entry('a.ts', 'M', { added: 4, removed: 2 }),
        entry('b.ts'),
        entry('c.ts', 'U', { added: 1, removed: 0 }),
      ]),
    ).toEqual({ added: 5, removed: 2 })
    expect(totalCounts([entry('a.ts')])).toBeNull()
  })
})
