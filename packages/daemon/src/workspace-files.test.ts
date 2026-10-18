import { createHash } from 'node:crypto'
import {
  chmodSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  realpathSync,
  rmSync,
  statSync,
  symlinkSync,
  writeFileSync,
} from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { MAX_FILE_BYTES, OutsideWorkspaceError } from './git.js'
import {
  FileChangedOnDiskError,
  FileTooLargeError,
  GitFolderError,
  listFolder,
  readWorkspaceFile,
  WorkspacePathError,
  WorkspacePathNotFoundError,
  writeWorkspaceFile,
} from './workspace-files.js'

const sha = (text: string | Buffer) => createHash('sha256').update(text).digest('hex')

let base: string
let root: string

beforeEach(() => {
  base = realpathSync(mkdtempSync(join(tmpdir(), 'natamux-files-')))
  root = join(base, 'worktree')
  mkdirSync(root)
})

afterEach(() => {
  rmSync(base, { recursive: true, force: true })
})

describe('listFolder', () => {
  it('lists folders first, then files, by name with numbers in order', async () => {
    mkdirSync(join(root, 'src'))
    mkdirSync(join(root, 'docs'))
    writeFileSync(join(root, 'b10.ts'), '')
    writeFileSync(join(root, 'b2.ts'), '')
    writeFileSync(join(root, '.env'), '')

    expect(await listFolder(root, '')).toEqual([
      { name: 'docs', kind: 'folder' },
      { name: 'src', kind: 'folder' },
      { name: '.env', kind: 'file' },
      { name: 'b2.ts', kind: 'file' },
      { name: 'b10.ts', kind: 'file' },
    ])
  })

  it('lists one folder only', async () => {
    mkdirSync(join(root, 'src', 'deep'), { recursive: true })
    writeFileSync(join(root, 'src', 'a.ts'), '')
    writeFileSync(join(root, 'src', 'deep', 'b.ts'), '')

    expect(await listFolder(root, 'src')).toEqual([
      { name: 'deep', kind: 'folder' },
      { name: 'a.ts', kind: 'file' },
    ])
  })

  it('never lists .git, as a folder or as the file of a worktree', async () => {
    mkdirSync(join(root, '.git'))
    writeFileSync(join(root, 'a.ts'), '')
    mkdirSync(join(root, 'sub'))
    writeFileSync(join(root, 'sub', '.git'), 'gitdir: ../x')

    expect(await listFolder(root, '')).toEqual([
      { name: 'sub', kind: 'folder' },
      { name: 'a.ts', kind: 'file' },
    ])
    expect(await listFolder(root, 'sub')).toEqual([])
  })

  it('lists a link by what it points to, and leaves out a link that leaves the worktree', async () => {
    mkdirSync(join(root, 'src'))
    writeFileSync(join(root, 'a.ts'), '')
    writeFileSync(join(base, 'secret.txt'), 'secret')
    symlinkSync(join(root, 'src'), join(root, 'src-link'))
    symlinkSync(join(root, 'a.ts'), join(root, 'a-link.ts'))
    symlinkSync(join(base, 'secret.txt'), join(root, 'out.txt'))
    symlinkSync(join(root, 'gone'), join(root, 'broken'))

    expect(await listFolder(root, '')).toEqual([
      { name: 'src', kind: 'folder' },
      { name: 'src-link', kind: 'folder' },
      { name: 'a-link.ts', kind: 'file' },
      { name: 'a.ts', kind: 'file' },
    ])
  })

  it('refuses a folder outside the worktree', async () => {
    mkdirSync(join(base, 'other'))
    symlinkSync(join(base, 'other'), join(root, 'other'))

    await expect(listFolder(root, '..')).rejects.toThrow(OutsideWorkspaceError)
    await expect(listFolder(root, base)).rejects.toThrow(OutsideWorkspaceError)
    await expect(listFolder(root, 'other')).rejects.toThrow(OutsideWorkspaceError)
  })

  it('refuses the .git folder', async () => {
    mkdirSync(join(root, '.git'))
    await expect(listFolder(root, '.git')).rejects.toThrow(GitFolderError)
    await expect(listFolder(root, 'src/../.git')).rejects.toThrow(GitFolderError)
  })

  it('says when the folder is missing or is a file', async () => {
    writeFileSync(join(root, 'a.ts'), '')
    await expect(listFolder(root, 'nope')).rejects.toThrow(WorkspacePathNotFoundError)
    await expect(listFolder(root, 'a.ts')).rejects.toThrow(WorkspacePathError)
  })
})

describe('readWorkspaceFile', () => {
  it('keeps a byte order mark, so a save writes it back', async () => {
    writeFileSync(join(root, 'bom.txt'), '\uFEFFhello')
    expect((await readWorkspaceFile(root, 'bom.txt')).text).toBe('\uFEFFhello')
  })

  it('reads the text with its sha256 as the version', async () => {
    mkdirSync(join(root, 'src'))
    writeFileSync(join(root, 'src', 'a.ts'), 'const a = 1\n')

    expect(await readWorkspaceFile(root, 'src/a.ts')).toEqual({
      path: 'src/a.ts',
      text: 'const a = 1\n',
      binary: false,
      tooLarge: false,
      version: sha('const a = 1\n'),
    })
  })

  it('sends no text for a binary, a not UTF-8 or a too large file', async () => {
    writeFileSync(join(root, 'image.bin'), Buffer.from([1, 0, 2]))
    writeFileSync(join(root, 'latin1.txt'), Buffer.from([0x63, 0x61, 0x66, 0xe9]))
    expect(await readWorkspaceFile(root, 'latin1.txt')).toMatchObject({ text: null, binary: true })
    writeFileSync(join(root, 'big.txt'), 'a'.repeat(MAX_FILE_BYTES + 1))

    expect(await readWorkspaceFile(root, 'image.bin')).toMatchObject({
      text: null,
      binary: true,
      tooLarge: false,
    })
    expect(await readWorkspaceFile(root, 'big.txt')).toMatchObject({
      text: null,
      binary: false,
      tooLarge: true,
    })
  })

  it('refuses paths outside the worktree, inside .git, missing, or a folder', async () => {
    writeFileSync(join(base, 'secret.txt'), 'secret')
    symlinkSync(join(base, 'secret.txt'), join(root, 'link.txt'))
    mkdirSync(join(root, '.git'))
    writeFileSync(join(root, '.git', 'config'), '')
    mkdirSync(join(root, 'src'))

    await expect(readWorkspaceFile(root, '../secret.txt')).rejects.toThrow(OutsideWorkspaceError)
    await expect(readWorkspaceFile(root, join(base, 'secret.txt'))).rejects.toThrow(
      OutsideWorkspaceError,
    )
    await expect(readWorkspaceFile(root, 'link.txt')).rejects.toThrow(OutsideWorkspaceError)
    await expect(readWorkspaceFile(root, '.git/config')).rejects.toThrow(GitFolderError)
    await expect(readWorkspaceFile(root, 'nope.ts')).rejects.toThrow(WorkspacePathNotFoundError)
    await expect(readWorkspaceFile(root, 'src')).rejects.toThrow(WorkspacePathError)
  })

  it('refuses a link that points into .git', async () => {
    mkdirSync(join(root, '.git'))
    writeFileSync(join(root, '.git', 'config'), '')
    symlinkSync(join(root, '.git', 'config'), join(root, 'config'))

    await expect(readWorkspaceFile(root, 'config')).rejects.toThrow(GitFolderError)
  })
})

describe('writeWorkspaceFile', () => {
  it('writes the text in place, keeps the mode, and answers the new version', async () => {
    const path = join(root, 'run.sh')
    writeFileSync(path, 'echo a\n')
    chmodSync(path, 0o755)

    const saved = await writeWorkspaceFile(root, 'run.sh', 'echo b\n', sha('echo a\n'))

    expect(saved).toEqual({ path: 'run.sh', version: sha('echo b\n') })
    expect(readFileSync(path, 'utf8')).toBe('echo b\n')
    expect(statSync(path).mode & 0o777).toBe(0o755)
  })

  it('does not replace a file that changed after it was read', async () => {
    const path = join(root, 'a.ts')
    writeFileSync(path, 'agent text')

    await expect(writeWorkspaceFile(root, 'a.ts', 'mine', sha('old text'))).rejects.toThrow(
      FileChangedOnDiskError,
    )
    expect(readFileSync(path, 'utf8')).toBe('agent text')
  })

  it('refuses a text over the size limit', async () => {
    writeFileSync(join(root, 'a.ts'), 'a')
    await expect(
      writeWorkspaceFile(root, 'a.ts', 'a'.repeat(MAX_FILE_BYTES + 1), sha('a')),
    ).rejects.toThrow(FileTooLargeError)
  })

  it('refuses a missing file, a folder, .git and paths outside', async () => {
    writeFileSync(join(base, 'secret.txt'), 'secret')
    symlinkSync(join(base, 'secret.txt'), join(root, 'link.txt'))
    mkdirSync(join(root, '.git'))
    writeFileSync(join(root, '.git', 'HEAD'), 'ref')
    mkdirSync(join(root, 'src'))

    await expect(writeWorkspaceFile(root, 'new.ts', 'x', sha(''))).rejects.toThrow(
      WorkspacePathNotFoundError,
    )
    await expect(writeWorkspaceFile(root, 'src', 'x', sha(''))).rejects.toThrow(WorkspacePathError)
    await expect(writeWorkspaceFile(root, '.git/HEAD', 'x', sha('ref'))).rejects.toThrow(
      GitFolderError,
    )
    await expect(writeWorkspaceFile(root, 'link.txt', 'x', sha('secret'))).rejects.toThrow(
      OutsideWorkspaceError,
    )
    await expect(writeWorkspaceFile(root, '../secret.txt', 'x', sha('secret'))).rejects.toThrow(
      OutsideWorkspaceError,
    )
    expect(readFileSync(join(base, 'secret.txt'), 'utf8')).toBe('secret')
  })
})
