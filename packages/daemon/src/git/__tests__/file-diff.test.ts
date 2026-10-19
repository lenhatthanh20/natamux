import { rmSync, symlinkSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import type { ChangeEntry, ChangeStatus, ImageSide } from '@natamux/core'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { createTestRepo, runGit, type TestRepo } from '../../__tests__/git-repo.fixture.js'
import { OutsideWorkspaceError } from '../exec.js'
import {
  ImageSideMissingError,
  ImageTooLargeError,
  MAX_IMAGE_BYTES,
  NotAnImageError,
  readFileDiff,
  readFileImage,
} from '../file-diff.js'
import { addWorktree } from '../worktrees.js'

let repo: TestRepo
let worktreePath: string

beforeEach(() => {
  repo = createTestRepo()
  worktreePath = join(repo.root, 'worktrees', 'T-0001')
})

afterEach(() => {
  repo.cleanup()
})

describe('readFileDiff', () => {
  async function prepare(): Promise<void> {
    await addWorktree(repo.dir, worktreePath, 'natamux/t-0001-example')
    writeFileSync(join(worktreePath, 'tracked.txt'), 'one\n')
    runGit(worktreePath, 'add', 'tracked.txt')
    runGit(worktreePath, 'commit', '-m', 'add tracked')
  }

  const change = (
    path: string,
    status: ChangeStatus,
    originalPath: string | null = null,
  ): ChangeEntry => ({ path, status, originalPath })

  it('gives the last commit and the file now for a modified file', async () => {
    await prepare()
    writeFileSync(join(worktreePath, 'tracked.txt'), 'two\n')

    expect(await readFileDiff(worktreePath, change('tracked.txt', 'M'))).toEqual({
      path: 'tracked.txt',
      status: 'M',
      original: 'one\n',
      modified: 'two\n',
      binary: false,
      tooLarge: false,
    })
  })

  it('has no original for a file that is not tracked yet', async () => {
    await prepare()
    writeFileSync(join(worktreePath, 'new.txt'), 'new\n')

    const diff = await readFileDiff(worktreePath, change('new.txt', 'U'))

    expect(diff.original).toBeNull()
    expect(diff.modified).toBe('new\n')
  })

  it('has no modified side for a deleted file', async () => {
    await prepare()
    rmSync(join(worktreePath, 'tracked.txt'))

    const diff = await readFileDiff(worktreePath, change('tracked.txt', 'D'))

    expect(diff.original).toBe('one\n')
    expect(diff.modified).toBeNull()
  })

  it('takes the original of a renamed file from its old path', async () => {
    await prepare()
    runGit(worktreePath, 'mv', 'tracked.txt', 'moved.txt')

    const diff = await readFileDiff(worktreePath, change('moved.txt', 'R', 'tracked.txt'))

    expect(diff.original).toBe('one\n')
    expect(diff.modified).toBe('one\n')
  })

  it('does not send the text of a binary file', async () => {
    await prepare()
    writeFileSync(join(worktreePath, 'image.bin'), Buffer.from([1, 2, 0, 3]))

    expect(await readFileDiff(worktreePath, change('image.bin', 'U'))).toMatchObject({
      binary: true,
      original: null,
      modified: null,
    })
  })

  it('does not send a file that is too large', async () => {
    await prepare()
    writeFileSync(join(worktreePath, 'big.txt'), 'a'.repeat(1_000_001))

    expect(await readFileDiff(worktreePath, change('big.txt', 'U'))).toMatchObject({
      tooLarge: true,
      original: null,
      modified: null,
    })
  })

  it('refuses a link that leaves the worktree', async () => {
    await prepare()
    const secret = join(repo.root, 'secret.txt')
    writeFileSync(secret, 'secret')
    symlinkSync(secret, join(worktreePath, 'link.txt'))

    await expect(readFileDiff(worktreePath, change('link.txt', 'U'))).rejects.toThrow(
      OutsideWorkspaceError,
    )
  })
})

describe('readFileImage', () => {
  const PNG_HEAD = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0x0d]
  const read = (path: string, status: ChangeStatus, side: ImageSide) =>
    readFileImage(worktreePath, { path, status, originalPath: null }, side)

  async function prepare(): Promise<void> {
    await addWorktree(repo.dir, worktreePath, 'natamux/t-0001-example')
    writeFileSync(join(worktreePath, 'logo.png'), Buffer.from([...PNG_HEAD, 1]))
    runGit(worktreePath, 'add', 'logo.png')
    runGit(worktreePath, 'commit', '-m', 'add logo')
  }

  it('gives both sides of a modified image', async () => {
    await prepare()
    writeFileSync(join(worktreePath, 'logo.png'), Buffer.from([...PNG_HEAD, 2]))

    const before = await read('logo.png', 'M', 'original')
    const after = await read('logo.png', 'M', 'modified')

    expect(before.type).toBe('image/png')
    expect([...before.bytes]).toEqual([...PNG_HEAD, 1])
    expect(after.type).toBe('image/png')
    expect([...after.bytes]).toEqual([...PNG_HEAD, 2])
  })

  it('has no original for a new image, and no modified side for a deleted one', async () => {
    await prepare()
    writeFileSync(join(worktreePath, 'new.png'), Buffer.from(PNG_HEAD))

    await expect(read('new.png', 'A', 'original')).rejects.toThrow(ImageSideMissingError)
    await expect(read('logo.png', 'D', 'modified')).rejects.toThrow(ImageSideMissingError)
  })

  it('refuses an image that is too large', async () => {
    await prepare()
    writeFileSync(join(worktreePath, 'big.png'), Buffer.alloc(MAX_IMAGE_BYTES + 1))

    await expect(read('big.png', 'U', 'modified')).rejects.toThrow(ImageTooLargeError)
  })

  it('refuses a link that leaves the worktree', async () => {
    await prepare()
    const secret = join(repo.root, 'secret.png')
    writeFileSync(secret, Buffer.from(PNG_HEAD))
    symlinkSync(secret, join(worktreePath, 'link.png'))

    await expect(read('link.png', 'U', 'modified')).rejects.toThrow(OutsideWorkspaceError)
  })

  it('refuses a file that is not an image', async () => {
    await prepare()
    writeFileSync(join(worktreePath, 'a.ts'), 'export {}\n')

    await expect(read('a.ts', 'U', 'modified')).rejects.toThrow(NotAnImageError)
  })
})
