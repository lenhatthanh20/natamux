import type { ReviewComment } from '@natamux/core'
import { useMemo, useRef, useState } from 'react'
import { commentsOfFile } from './review-comments'
import type { RevealLine, ZoneActions } from './review-zones'
import type { useReviewComments } from './useReviewComments'

type Review = ReturnType<typeof useReviewComments>

// What the diff shows of the comments: the ones of the shown file, the buttons of their zones, and
// the line a click in the list moves to.
export function useReviewView(
  review: Review,
  shownPath: string | null,
  choose: (path: string) => void,
) {
  const [revealed, setRevealed] = useState<(RevealLine & { path: string }) | null>(null)
  // Stable, so the zones in the diff are not built again on each render.
  const latest = useRef(review)
  latest.current = review
  const zoneActions = useMemo<ZoneActions>(
    () => ({
      onEdit: (id) => latest.current.setEditingId(id),
      onDelete: (id) => void latest.current.remove(id),
      onResolve: (id) => void latest.current.resolve(id),
      onReopen: (id) => void latest.current.reopen(id),
    }),
    [],
  )
  const fileComments = useMemo(
    () => commentsOfFile(review.comments, shownPath ?? '', review.showResolved),
    [review.comments, review.showResolved, shownPath],
  )
  // A comment of the list: its file, and its line when it has one.
  const open = (comment: ReviewComment) => {
    choose(comment.path)
    setRevealed(
      comment.start === 0
        ? null
        : { path: comment.path, side: comment.side, line: comment.start, n: Date.now() },
    )
  }
  return {
    fileComments,
    zoneActions,
    open,
    reveal: revealed !== null && revealed.path === shownPath ? revealed : null,
  }
}
