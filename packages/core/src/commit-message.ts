import { z } from 'zod'
import type { ChangeEntry } from './changes.js'

// The usual limit of the first line of a commit message.
export const COMMIT_SUBJECT_MAX = 72
const MAX_LISTED_FILES = 20

// A commit message drafted by the daemon. `draft`: the agent could not write it, so it is the
// simple draft, and `reason` says why.
export const commitMessageDraftSchema = z.object({
  message: z.string().min(1),
  source: z.enum(['agent', 'draft']),
  agent: z.string().min(1),
  reason: z.string().optional(),
})

export type CommitMessageDraft = z.infer<typeof commitMessageDraftSchema>

// Cuts a first line to the limit, at the last space when there is one.
export function cutCommitSubject(line: string): string {
  if (line.length <= COMMIT_SUBJECT_MAX) {
    return line
  }
  const space = line.lastIndexOf(' ', COMMIT_SUBJECT_MAX)
  return (space > 0 ? line.slice(0, space) : line.slice(0, COMMIT_SUBJECT_MAX)).trimEnd()
}

type DraftChange = Pick<ChangeEntry, 'path' | 'status' | 'originalPath'>

function oneFile(change: DraftChange): string {
  switch (change.status) {
    case 'A':
    case 'U':
      return `Add ${change.path}`
    case 'D':
      return `Delete ${change.path}`
    case 'R':
      return `Rename ${change.originalPath ?? change.path} to ${change.path}`
    default:
      return `Update ${change.path}`
  }
}

// A message from the task title and the changed files, for when no agent can write one.
export function simpleCommitDraft(title: string, changes: readonly DraftChange[]): string {
  if (changes.length === 1 && changes[0] !== undefined) {
    return cutCommitSubject(oneFile(changes[0]))
  }
  const subject = cutCommitSubject(title.trim() || `Update ${changes.length} files`)
  const lines = changes
    .slice(0, MAX_LISTED_FILES)
    .map((change) =>
      change.status === 'R' && change.originalPath !== null
        ? `R ${change.originalPath} -> ${change.path}`
        : `${change.status} ${change.path}`,
    )
  const more = changes.length - MAX_LISTED_FILES
  if (more > 0) {
    lines.push(`and ${more} more files`)
  }
  return lines.length === 0 ? subject : `${subject}\n\n${lines.join('\n')}`
}
