import type { Task } from '@natamux/core'
import { useEffect, useRef, useState } from 'react'

type NeedsYouAnnouncerProps = {
  // The tasks that wait for the user.
  waiting: readonly Task[]
  // False until the task list has loaded: an empty list before that is not "nothing waits".
  loaded: boolean
}

// Tells screen readers, politely, when a task starts to wait for the user.
export function NeedsYouAnnouncer({ waiting, loaded }: NeedsYouAnnouncerProps) {
  const [announcement, setAnnouncement] = useState('')
  // The tasks that waited already. Null before the first loaded render: those are not news.
  const known = useRef<Set<string> | null>(null)
  useEffect(() => {
    if (!loaded) {
      return
    }
    const fresh =
      known.current === null ? [] : waiting.filter((task) => !known.current?.has(task.id))
    known.current = new Set(waiting.map((task) => task.id))
    if (fresh.length > 0) {
      setAnnouncement(fresh.map((task) => `${task.title} needs you`).join('. '))
    }
  }, [waiting, loaded])

  return (
    <p role="status" aria-live="polite" className="sr-only">
      {announcement}
    </p>
  )
}
