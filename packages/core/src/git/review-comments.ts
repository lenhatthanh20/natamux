import { z } from 'zod'
import { taskSchema } from '../tasks/task.js'

// A comment's text: at most this many characters, after trimming (same as the daemon).
export const MAX_COMMENT_BODY = 5000

export const commentSideSchema = z.enum(['old', 'new'])
export type CommentSide = z.infer<typeof commentSideSchema>

// A draft waits for the next send; a sent comment waits for the person to resolve it.
export const commentStateSchema = z.enum(['draft', 'sent', 'resolved'])
export type CommentState = z.infer<typeof commentStateSchema>

// `start` and `end` are 1-based lines of `side`; both 0 means the whole file. `outdated` is absent
// when false.
export const reviewCommentSchema = z.object({
  id: z.string().min(1),
  path: z.string().min(1),
  side: commentSideSchema,
  start: z.number().int().nonnegative(),
  end: z.number().int().nonnegative(),
  quote: z.string(),
  body: z.string(),
  state: commentStateSchema,
  createdAt: z.string(),
  sentAt: z.string().optional(),
  resolvedAt: z.string().optional(),
  outdated: z.boolean().optional(),
})
export type ReviewComment = z.infer<typeof reviewCommentSchema>

export const reviewCommentListSchema = z.object({ comments: z.array(reviewCommentSchema) })
export const reviewCommentAnswerSchema = z.object({ comment: reviewCommentSchema })
export const sendReviewAnswerSchema = z.object({
  task: taskSchema,
  comments: z.array(reviewCommentSchema),
})
export type SendReviewAnswer = z.infer<typeof sendReviewAnswerSchema>

export type NewReviewComment = {
  path: string
  side: CommentSide
  start: number
  end: number
  body: string
}

export type ReviewCommentPatch = { body?: string; state?: 'resolved' | 'sent' }
