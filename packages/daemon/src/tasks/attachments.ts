import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  statSync,
  writeFileSync,
} from 'node:fs'
import { join } from 'node:path'
import {
  ATTACHMENTS_DIR,
  type Attachment,
  type AttachmentUpload,
  attachmentLimitError,
  attachmentNames,
  formatBytes,
  previewTypeOf,
  safeAttachmentName,
  type Task,
} from '@natamux/core'
import { archiveDir, STAGE_DIR } from './task-files.js'

export type AttachmentFile = { name: string; bytes: Buffer }

export class AttachmentError extends Error {
  constructor(message: string) {
    super(message)
    this.name = 'AttachmentError'
  }
}

// The attachments are written here once, when the task is made, and stay after its workspace is
// removed. A worktree gets a copy.
export function attachmentsDir(dataDir: string, taskId: string): string {
  return join(archiveDir(dataDir, taskId), ATTACHMENTS_DIR)
}

// Where an agent finds an attachment, from the root of the task's worktree.
export function attachmentPath(name: string): string {
  return `${STAGE_DIR}/${ATTACHMENTS_DIR}/${name}`
}

// Checked before anything is written, so a file over a limit makes no task.
export function decodeAttachments(uploads: readonly AttachmentUpload[]): AttachmentFile[] {
  const decoded = uploads.map((upload) => ({
    name: upload.name,
    bytes: Buffer.from(upload.data, 'base64'),
  }))
  const sizes = decoded.map(({ name, bytes }) => ({ name, size: bytes.length }))
  const error = attachmentLimitError(sizes)
  if (error !== undefined) {
    throw new AttachmentError(error)
  }
  const names = attachmentNames(decoded.map((file) => file.name))
  return decoded.map((file, index) => ({
    name: names[index] ?? safeAttachmentName(file.name),
    bytes: file.bytes,
  }))
}

// The names must come from decodeAttachments or from an earlier task, so they are safe.
export function saveAttachments(
  dataDir: string,
  taskId: string,
  files: readonly AttachmentFile[],
): void {
  if (files.length === 0) {
    return
  }
  const dir = attachmentsDir(dataDir, taskId)
  mkdirSync(dir, { recursive: true })
  for (const file of files) {
    writeFileSync(join(dir, safeAttachmentName(file.name)), file.bytes)
  }
}

export function listAttachments(dataDir: string, taskId: string): Attachment[] {
  const dir = attachmentsDir(dataDir, taskId)
  if (!existsSync(dir)) {
    return []
  }
  return readdirSync(dir, { withFileTypes: true })
    .filter((entry) => entry.isFile() && safeAttachmentName(entry.name) === entry.name)
    .map((entry) => entry.name)
    .sort()
    .map((name) => ({
      name,
      size: statSync(join(dir, name)).size,
      type: previewTypeOf(name),
    }))
}

// The bytes of one attachment; undefined when the task has none of that name.
export function readAttachment(dataDir: string, taskId: string, name: string): Buffer | undefined {
  if (safeAttachmentName(name) !== name) {
    throw new AttachmentError(`"${name}" is not the name of an attachment`)
  }
  const path = join(attachmentsDir(dataDir, taskId), name)
  return existsSync(path) ? readFileSync(path) : undefined
}

// All attachments of a task, for a copy of the task.
export function readAttachments(dataDir: string, taskId: string): AttachmentFile[] {
  const dir = attachmentsDir(dataDir, taskId)
  return listAttachments(dataDir, taskId).map(({ name }) => ({
    name,
    bytes: readFileSync(join(dir, name)),
  }))
}

// Puts the attachments into a new worktree, next to the stage files, so its agent finds them.
export function restoreAttachments(dataDir: string, task: Pick<Task, 'id' | 'workspace'>): void {
  const attachments = listAttachments(dataDir, task.id)
  if (task.workspace === null || attachments.length === 0) {
    return
  }
  const from = attachmentsDir(dataDir, task.id)
  const to = join(task.workspace, STAGE_DIR, ATTACHMENTS_DIR)
  mkdirSync(to, { recursive: true })
  for (const { name } of attachments) {
    copyFileSync(join(from, name), join(to, name))
  }
}

const ATTACHMENTS_HEADING = /^#{1,6}[ \t]+Attachments[ \t]*$/im

export function attachmentSection(attachments: readonly Attachment[]): string {
  return [
    '## Attachments',
    '',
    'Files attached to the task, by their path from the root of the worktree:',
    '',
    ...attachments.map(
      (attachment) => `- \`${attachmentPath(attachment.name)}\` (${formatBytes(attachment.size)})`,
    ),
    '',
  ].join('\n')
}

// The context with an Attachments section at its end, unless it has one already. An agent that
// wrote the context may have left it out.
export function withAttachmentSection(text: string, attachments: readonly Attachment[]): string {
  if (attachments.length === 0 || ATTACHMENTS_HEADING.test(text)) {
    return text
  }
  const gap = text === '' || text.endsWith('\n\n') ? '' : text.endsWith('\n') ? '\n' : '\n\n'
  return `${text}${gap}${attachmentSection(attachments)}`
}
