import { z } from 'zod'
import { imageTypeOf } from './changes.js'

// Files attached to a task when it starts, for its agents to read. They are kept next to the
// stage files, in this folder.
export const ATTACHMENTS_DIR = 'attachments'
export const MAX_ATTACHMENT_COUNT = 20
export const MAX_ATTACHMENT_SIZE = 20_000_000
export const MAX_TASK_ATTACHMENT_SIZE = 50_000_000
const MAX_NAME_LENGTH = 100
// Only a short extension is kept when a long name is cut.
const MAX_EXTENSION_LENGTH = 20

const MB = 1_000_000
const FILE_LIMIT = `${MAX_ATTACHMENT_SIZE / MB} MB`
const TASK_LIMIT = `${MAX_TASK_ATTACHMENT_SIZE / MB} MB`

// The length of the base64 text of that many bytes.
export function base64Length(bytes: number): number {
  return Math.ceil(bytes / 3) * 4
}

export const attachmentSchema = z.object({
  name: z.string().min(1),
  size: z.number().int().nonnegative(),
  // The image type the dashboard may show; null for a file that only downloads.
  type: z.string().nullable(),
})

export type Attachment = z.infer<typeof attachmentSchema>

// A file sent with a new task, as base64, so it fits in the JSON body of the request.
export const attachmentUploadSchema = z.object({
  name: z.string().min(1).max(255),
  data: z.string().max(base64Length(MAX_ATTACHMENT_SIZE)),
})

export const attachmentUploadsSchema = z.array(attachmentUploadSchema).max(MAX_ATTACHMENT_COUNT)

export type AttachmentUpload = z.infer<typeof attachmentUploadSchema>

// SVG is left out: it can hold scripts.
export function previewTypeOf(name: string): string | null {
  const type = imageTypeOf(name)
  return type === 'image/svg+xml' ? null : type
}

function cut(name: string): string {
  if (name.length <= MAX_NAME_LENGTH) {
    return name
  }
  const dot = name.lastIndexOf('.')
  const extension = dot > 0 && name.length - dot <= MAX_EXTENSION_LENGTH ? name.slice(dot) : ''
  return name.slice(0, MAX_NAME_LENGTH - extension.length) + extension
}

// A file name that cannot leave the attachments folder or start a hidden file, and that reads the
// same in a shell: only letters, digits, dots, dashes and underscores.
export function safeAttachmentName(name: string): string {
  const base = name.split(/[\\/]/).pop() ?? ''
  const safe = cut(base.replace(/[^\w.-]/g, '-').replace(/^[.-]+/, ''))
  return safe === '' ? 'file' : safe
}

function numbered(name: string, number: number): string {
  const dot = name.lastIndexOf('.')
  return dot > 0 ? `${name.slice(0, dot)}-${number}${name.slice(dot)}` : `${name}-${number}`
}

// Safe names for files in this order; a name already used gets a number: a.png, a-2.png. Names
// are compared without case, as the disks of macOS and Windows do.
export function attachmentNames(names: readonly string[]): string[] {
  const taken = new Set<string>()
  return names.map((name) => {
    const safe = safeAttachmentName(name)
    let unique = safe
    for (let number = 2; taken.has(unique.toLowerCase()); number += 1) {
      unique = numbered(safe, number)
    }
    taken.add(unique.toLowerCase())
    return unique
  })
}

export type SizedFile = { name: string; size: number }
export type RefusedFile = { name: string; reason: string }

// Splits added files into those that fit next to the ones a task already has and those over a
// limit, with the reason for a person.
export function fitAttachments<T extends SizedFile>(
  current: readonly SizedFile[],
  added: readonly T[],
): { kept: T[]; refused: RefusedFile[] } {
  let count = current.length
  let total = current.reduce((sum, file) => sum + file.size, 0)
  const kept: T[] = []
  const refused: RefusedFile[] = []
  for (const file of added) {
    let reason: string | undefined
    if (file.size > MAX_ATTACHMENT_SIZE) {
      reason = `${file.name} is larger than ${FILE_LIMIT}, the most for one file`
    } else if (count >= MAX_ATTACHMENT_COUNT) {
      reason = `${file.name} is not attached: a task takes at most ${MAX_ATTACHMENT_COUNT} files`
    } else if (total + file.size > MAX_TASK_ATTACHMENT_SIZE) {
      reason = `${file.name} is not attached: the files of a task take at most ${TASK_LIMIT} together`
    }
    if (reason === undefined) {
      kept.push(file)
      count += 1
      total += file.size
    } else {
      refused.push({ name: file.name, reason })
    }
  }
  return { kept, refused }
}

// Why these files cannot all be attached to one task; undefined when they can.
export function attachmentLimitError(files: readonly SizedFile[]): string | undefined {
  return fitAttachments([], files).refused[0]?.reason
}

export function formatBytes(bytes: number): string {
  if (bytes < 1000) {
    return `${bytes} B`
  }
  if (bytes < MB) {
    return `${(bytes / 1000).toFixed(1)} KB`
  }
  return `${(bytes / MB).toFixed(1)} MB`
}
