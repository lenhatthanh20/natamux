import type { CommentState, ReviewComment } from '@natamux/core'
import { linesLabel } from '../../lib/changes'

export type CommentAction = 'edit' | 'delete' | 'resolve' | 'reopen'

// "line 12", "lines 12–14" or "whole file", with the side when it is the old text.
export function commentLabel(comment: Pick<ReviewComment, 'start' | 'end' | 'side'>): string {
  const where = comment.start === 0 ? 'whole file' : linesLabel(comment)
  return comment.side === 'old' && comment.start !== 0 ? `${where} (before the change)` : where
}

export function stateLabel(state: CommentState): string {
  return state === 'draft' ? 'Draft' : state === 'sent' ? 'Sent' : 'Resolved'
}

// Draft and sent comments are still for the agent.
export function openCount(comments: readonly ReviewComment[]): number {
  return comments.filter((comment) => comment.state !== 'resolved').length
}

export function draftCount(comments: readonly ReviewComment[]): number {
  return comments.filter((comment) => comment.state === 'draft').length
}

export function visibleComments(
  comments: readonly ReviewComment[],
  showResolved: boolean,
): ReviewComment[] {
  return comments.filter((comment) => showResolved || comment.state !== 'resolved')
}

// The comments to draw inside the diff of one file.
export function commentsOfFile(
  comments: readonly ReviewComment[],
  path: string,
  showResolved: boolean,
): ReviewComment[] {
  return visibleComments(comments, showResolved).filter((comment) => comment.path === path)
}

export function actionsFor(state: CommentState): CommentAction[] {
  if (state === 'draft') {
    return ['edit', 'delete']
  }
  return state === 'sent' ? ['resolve', 'delete'] : ['reopen']
}

// Files in path order, comments in line order inside each.
export function groupByFile(
  comments: readonly ReviewComment[],
  showResolved: boolean,
): { path: string; comments: ReviewComment[] }[] {
  const groups = new Map<string, ReviewComment[]>()
  for (const comment of visibleComments(comments, showResolved)) {
    groups.set(comment.path, [...(groups.get(comment.path) ?? []), comment])
  }
  return [...groups.entries()]
    .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
    .map(([path, list]) => ({
      path,
      comments: list.sort(
        (a, b) => a.start - b.start || a.id.localeCompare(b.id, 'en', { numeric: true }),
      ),
    }))
}

// The text of "Send N comments".
export function sendLabel(drafts: number): string {
  return drafts === 1 ? 'Send 1 comment' : `Send ${drafts} comments`
}
