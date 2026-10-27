import type {
  NewReviewComment,
  ReviewComment,
  ReviewCommentPatch,
  SendReviewAnswer,
} from '@natamux/core'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { useAppEvent } from '../../lib/app-events'
import { FALLBACK_POLL_MS } from '../../lib/live'
import { draftCount } from './review-comments'

// The calls of the review comments of one task. The daemon's `review-comments.changed` message
// reads the list again, so a call needs no refresh of its own.
export type ReviewCommentsApi = {
  list: () => Promise<ReviewComment[]>
  add: (input: NewReviewComment) => Promise<ReviewComment>
  update: (id: string, patch: ReviewCommentPatch) => Promise<ReviewComment>
  remove: (id: string) => Promise<void>
  send: () => Promise<SendReviewAnswer>
}

const NONE: ReviewComment[] = []

export function useReviewComments(taskId: string, api: ReviewCommentsApi | undefined) {
  const queryClient = useQueryClient()
  const [showResolved, setShowResolved] = useState(false)
  // The draft whose text is being edited in the list.
  const [editingId, setEditingId] = useState<string | null>(null)
  const [sending, setSending] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const key = ['review-comments', taskId]
  const query = useQuery({
    queryKey: key,
    queryFn: () => (api === undefined ? Promise.resolve(NONE) : api.list()),
    enabled: api !== undefined,
    refetchInterval: FALLBACK_POLL_MS,
  })
  const comments = query.data ?? NONE
  const reread = () => queryClient.invalidateQueries({ queryKey: key })

  // Runs a call, shows its error, and reads the list again.
  const run = async (call: () => Promise<unknown>): Promise<boolean> => {
    setError(null)
    try {
      await call()
      return true
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : String(failure))
      return false
    } finally {
      void reread()
    }
  }

  const send = async () => {
    if (api === undefined || sending || draftCount(comments) === 0) {
      return
    }
    setSending(true)
    try {
      await run(() => api.send())
    } finally {
      setSending(false)
    }
  }

  useAppEvent('send-comments', () => void send())

  return {
    comments,
    showResolved,
    setShowResolved,
    editingId,
    setEditingId,
    sending,
    error,
    send,
    resolve: (id: string) => run(() => api?.update(id, { state: 'resolved' }) ?? Promise.resolve()),
    reopen: (id: string) => run(() => api?.update(id, { state: 'sent' }) ?? Promise.resolve()),
    remove: (id: string) => run(() => api?.remove(id) ?? Promise.resolve()),
    save: async (id: string, body: string) => {
      const saved = await run(() => api?.update(id, { body }) ?? Promise.resolve())
      if (saved) {
        setEditingId(null)
      }
      return saved
    },
    add:
      api === undefined ? undefined : (input: NewReviewComment) => api.add(input).finally(reread),
  }
}
