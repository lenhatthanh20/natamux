import { describe, expect, it } from 'vitest'
import {
  attachmentLimitError,
  attachmentNames,
  base64Length,
  fitAttachments,
  formatBytes,
  MAX_ATTACHMENT_COUNT,
  MAX_ATTACHMENT_SIZE,
  MAX_TASK_ATTACHMENT_SIZE,
  previewTypeOf,
  safeAttachmentName,
} from './attachments.js'

describe('safeAttachmentName', () => {
  it.each([
    ['login-error.png', 'login-error.png'],
    ['Screen Shot 2026-10-11 at 9.41.png', 'Screen-Shot-2026-10-11-at-9.41.png'],
    ['docs/spec.docx', 'spec.docx'],
    ['C:\\Users\\me\\report.pdf', 'report.pdf'],
    ['../../etc/passwd', 'passwd'],
    ['..', 'file'],
    ['.', 'file'],
    ['', 'file'],
    ['.env', 'env'],
    ['-rf.txt', 'rf.txt'],
    ['notes (1).md', 'notes--1-.md'],
    ['tài liệu.xlsx', 't-i-li-u.xlsx'],
  ])('makes %j safe as %j', (name, safe) => {
    expect(safeAttachmentName(name)).toBe(safe)
  })

  it('keeps a long name to 100 characters, with its extension', () => {
    const safe = safeAttachmentName(`${'a'.repeat(300)}.pdf`)
    expect(safe).toHaveLength(100)
    expect(safe.endsWith('a.pdf')).toBe(true)
  })

  it('gives back a name that is already safe as it is', () => {
    for (const name of ['a.png', 'report_v2.final.pdf', 'pasted-1.png']) {
      expect(safeAttachmentName(safeAttachmentName(name))).toBe(name)
    }
  })
})

describe('attachmentNames', () => {
  it('makes each name safe and gives a repeated name a number', () => {
    expect(
      attachmentNames(['screenshot.png', 'screenshot.png', 'a/screenshot.png', 'notes', 'notes']),
    ).toEqual([
      'screenshot.png',
      'screenshot-2.png',
      'screenshot-3.png',
      'notes',
      'notes-2',
    ])
  })

  it('counts names that differ only in case as the same', () => {
    expect(attachmentNames(['A.png', 'a.png'])).toEqual(['A.png', 'a-2.png'])
  })

  it('does not reuse a name that is taken by a later file', () => {
    expect(attachmentNames(['a.png', 'a.png', 'a-2.png'])).toEqual([
      'a.png',
      'a-2.png',
      'a-2-2.png',
    ])
  })
})

describe('fitAttachments', () => {
  const file = (name: string, size: number) => ({ name, size })

  it('keeps files within the limits', () => {
    const added = [file('a.png', 10), file('b.pdf', MAX_ATTACHMENT_SIZE)]
    expect(fitAttachments([], added)).toEqual({ kept: added, refused: [] })
  })

  it('refuses a file over the size of one file, and keeps the others', () => {
    const added = [file('big.mov', MAX_ATTACHMENT_SIZE + 1), file('a.png', 1)]
    const result = fitAttachments([], added)
    expect(result.kept).toEqual([file('a.png', 1)])
    expect(result.refused).toEqual([
      { name: 'big.mov', reason: 'big.mov is larger than 20 MB, the most for one file' },
    ])
  })

  it('refuses the files after the 20th', () => {
    const current = Array.from({ length: MAX_ATTACHMENT_COUNT - 1 }, (_, n) => file(`${n}.txt`, 1))
    const result = fitAttachments(current, [file('last.txt', 1), file('extra.txt', 1)])
    expect(result.kept.map((each) => each.name)).toEqual(['last.txt'])
    expect(result.refused).toEqual([
      { name: 'extra.txt', reason: 'extra.txt is not attached: a task takes at most 20 files' },
    ])
  })

  it('refuses a file that takes the total over 50 MB, and keeps a smaller one after it', () => {
    const current = [file('a.pdf', 20_000_000), file('b.pdf', 20_000_000)]
    const result = fitAttachments(current, [file('c.pdf', 15_000_000), file('d.png', 5_000_000)])
    expect(result.kept).toEqual([file('d.png', 5_000_000)])
    expect(result.refused).toEqual([
      {
        name: 'c.pdf',
        reason: 'c.pdf is not attached: the files of a task take at most 50 MB together',
      },
    ])
    expect(MAX_TASK_ATTACHMENT_SIZE).toBe(50_000_000)
  })
})

describe('attachmentLimitError', () => {
  it('is undefined for files within the limits', () => {
    expect(attachmentLimitError([{ name: 'a.png', size: 1 }])).toBeUndefined()
  })

  it('names the first file over a limit', () => {
    const files = Array.from({ length: MAX_ATTACHMENT_COUNT + 1 }, (_, n) => ({
      name: `f${n}.txt`,
      size: 1,
    }))
    expect(attachmentLimitError(files)).toBe(
      'f20.txt is not attached: a task takes at most 20 files',
    )
  })
})

describe('previewTypeOf', () => {
  it.each([
    ['a.png', 'image/png'],
    ['a.JPG', 'image/jpeg'],
    ['a.jpeg', 'image/jpeg'],
    ['a.gif', 'image/gif'],
    ['a.webp', 'image/webp'],
    ['a.svg', null],
    ['a.html', null],
    ['a.pdf', null],
    ['png', null],
  ])('gives %s the type %s', (name, type) => {
    expect(previewTypeOf(name)).toBe(type)
  })
})

describe('base64Length', () => {
  it('is the length of the base64 text of that many bytes', () => {
    // "a" is "YQ==", "abc" is "YWJj", "abcd" is "YWJjZA==".
    expect(base64Length(0)).toBe(0)
    expect(base64Length(1)).toBe(4)
    expect(base64Length(3)).toBe(4)
    expect(base64Length(4)).toBe(8)
  })
})

describe('formatBytes', () => {
  it('writes a size in B, KB or MB', () => {
    expect(formatBytes(512)).toBe('512 B')
    expect(formatBytes(12_300)).toBe('12.3 KB')
    expect(formatBytes(4_200_000)).toBe('4.2 MB')
  })
})
