import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { MAX_ATTACHMENT_SIZE } from '@natamux/core'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import {
  AttachmentError,
  attachmentSection,
  attachmentsDir,
  decodeAttachments,
  listAttachments,
  readAttachment,
  readAttachments,
  restoreAttachments,
  saveAttachments,
  withAttachmentSection,
} from './attachments.js'

let dataDir: string
let worktree: string

beforeEach(() => {
  dataDir = mkdtempSync(join(tmpdir(), 'natamux-attachments-'))
  worktree = mkdtempSync(join(tmpdir(), 'natamux-attachments-worktree-'))
})

afterEach(() => {
  rmSync(dataDir, { recursive: true, force: true })
  rmSync(worktree, { recursive: true, force: true })
})

function upload(name: string, text: string) {
  return { name, data: Buffer.from(text).toString('base64') }
}

describe('decodeAttachments', () => {
  it('decodes the files and makes their names safe and unique', () => {
    const files = decodeAttachments([
      upload('../shot.png', 'one'),
      upload('shot.png', 'two'),
      upload('My notes.md', '# Notes'),
    ])
    expect(files.map((file) => file.name)).toEqual(['shot.png', 'shot-2.png', 'My-notes.md'])
    expect(files.map((file) => file.bytes.toString())).toEqual(['one', 'two', '# Notes'])
  })

  it('keeps the bytes of a binary file as they are', () => {
    const bytes = Buffer.from([0, 255, 137, 80, 78, 71, 13, 10, 26, 10])
    const [file] = decodeAttachments([{ name: 'a.png', data: bytes.toString('base64') }])
    expect(file?.bytes.equals(bytes)).toBe(true)
  })

  it('refuses files over a limit, with the name of the file', () => {
    const data = Buffer.alloc(MAX_ATTACHMENT_SIZE + 1).toString('base64')
    const big = { name: 'big.bin', data }
    expect(() => decodeAttachments([upload('a.txt', 'a'), big])).toThrow(
      new AttachmentError('big.bin is larger than 20 MB, the most for one file'),
    )
  })

  it('refuses a 21st file', () => {
    const uploads = Array.from({ length: 21 }, (_, n) => upload(`f${n}.txt`, 'x'))
    expect(() => decodeAttachments(uploads)).toThrow(AttachmentError)
  })
})

describe('saveAttachments and listAttachments', () => {
  it('keeps the files in the task folder of the data folder and lists them by name', () => {
    const files = decodeAttachments([upload('b.pdf', 'pdf'), upload('a.png', 'image')])
    saveAttachments(dataDir, 'T-0001', files)

    const saved = join(dataDir, 'tasks', 'T-0001', 'attachments', 'a.png')
    expect(readFileSync(saved, 'utf8')).toBe('image')
    expect(listAttachments(dataDir, 'T-0001')).toEqual([
      { name: 'a.png', size: 5, type: 'image/png' },
      { name: 'b.pdf', size: 3, type: null },
    ])
  })

  it('lists nothing for a task without attachments, and writes no folder for none', () => {
    saveAttachments(dataDir, 'T-0001', [])
    expect(existsSync(attachmentsDir(dataDir, 'T-0001'))).toBe(false)
    expect(listAttachments(dataDir, 'T-0001')).toEqual([])
  })

  it('leaves out folders and files with names that are not safe', () => {
    const dir = attachmentsDir(dataDir, 'T-0001')
    mkdirSync(join(dir, 'inner'), { recursive: true })
    writeFileSync(join(dir, '.hidden'), 'x')
    writeFileSync(join(dir, 'ok.txt'), 'x')
    expect(listAttachments(dataDir, 'T-0001').map((each) => each.name)).toEqual(['ok.txt'])
  })
})

describe('readAttachment', () => {
  it('gives the bytes of an attachment, and undefined for a name the task does not have', () => {
    saveAttachments(dataDir, 'T-0001', decodeAttachments([upload('a.txt', 'hello')]))
    expect(readAttachment(dataDir, 'T-0001', 'a.txt')?.toString()).toBe('hello')
    expect(readAttachment(dataDir, 'T-0001', 'b.txt')).toBeUndefined()
  })

  it('refuses a name that could leave the attachments folder', () => {
    for (const name of ['../T-0001.md', '..', '.hidden', 'a b.txt']) {
      expect(() => readAttachment(dataDir, 'T-0001', name)).toThrow(AttachmentError)
    }
  })
})

describe('readAttachments', () => {
  it('reads every attachment, for a copy of the task', () => {
    const saved = decodeAttachments([upload('a.txt', 'A'), upload('b.txt', 'B')])
    saveAttachments(dataDir, 'T-0001', saved)
    const files = readAttachments(dataDir, 'T-0001')
    expect(files.map((file) => [file.name, file.bytes.toString()])).toEqual([
      ['a.txt', 'A'],
      ['b.txt', 'B'],
    ])
  })
})

describe('restoreAttachments', () => {
  it('copies the attachments into the stage folder of the worktree', () => {
    saveAttachments(dataDir, 'T-0001', decodeAttachments([upload('a.png', 'image')]))
    restoreAttachments(dataDir, { id: 'T-0001', workspace: worktree })
    const copy = join(worktree, '.natamux-task', 'attachments', 'a.png')
    expect(readFileSync(copy, 'utf8')).toBe('image')
    // The archive keeps its copy, so a removed workspace loses none.
    expect(listAttachments(dataDir, 'T-0001').map((each) => each.name)).toEqual(['a.png'])
  })

  it('does nothing for a task without attachments or without a worktree', () => {
    restoreAttachments(dataDir, { id: 'T-0001', workspace: worktree })
    expect(existsSync(join(worktree, '.natamux-task'))).toBe(false)
    saveAttachments(dataDir, 'T-0001', decodeAttachments([upload('a.png', 'image')]))
    expect(() => restoreAttachments(dataDir, { id: 'T-0001', workspace: null })).not.toThrow()
  })
})

describe('withAttachmentSection', () => {
  const attachments = [
    { name: 'login-error.png', size: 12_300, type: 'image/png' },
    { name: 'spec.docx', size: 512, type: null },
  ]
  const section = [
    '## Attachments',
    '',
    'Files attached to the task, by their path from the root of the worktree:',
    '',
    '- `.natamux-task/attachments/login-error.png` (12.3 KB)',
    '- `.natamux-task/attachments/spec.docx` (512 B)',
    '',
  ].join('\n')

  it('lists each file by its path from the worktree root, with its size', () => {
    expect(attachmentSection(attachments)).toBe(section)
  })

  it('adds the section after a blank line at the end of the context', () => {
    expect(withAttachmentSection('# Fix it\n\nNotes.\n', attachments)).toBe(
      `# Fix it\n\nNotes.\n\n${section}`,
    )
    expect(withAttachmentSection('# Fix it', attachments)).toBe(`# Fix it\n\n${section}`)
  })

  it('leaves a context with its own Attachments heading as it is', () => {
    const text = '# Fix it\n\n### attachments\n\n- the screenshot\n'
    expect(withAttachmentSection(text, attachments)).toBe(text)
  })

  it('leaves the context of a task without attachments as it is', () => {
    expect(withAttachmentSection('# Fix it\n', [])).toBe('# Fix it\n')
  })
})
