import type { Task, TaskState } from '@natamux/core'
import { useEffect, useRef, useState } from 'react'
import { notificationFor, statesById, tasksThatStartedWaiting } from '../lib/notify'

export type NotifyPermission = NotificationPermission | 'unsupported'

function currentPermission(): NotifyPermission {
  return typeof Notification === 'undefined' ? 'unsupported' : Notification.permission
}

// Shows a system notification when a task starts to wait while the dashboard is not in front.
export function useTaskNotifications(tasks: readonly Task[]) {
  const [permission, setPermission] = useState<NotifyPermission>(currentPermission)
  const seen = useRef<Map<string, TaskState> | null>(null)

  useEffect(() => {
    const previous = seen.current
    seen.current = statesById(tasks)
    if (previous === null || permission !== 'granted' || !document.hidden) {
      return
    }
    for (const task of tasksThatStartedWaiting(previous, tasks)) {
      const { title, body } = notificationFor(task)
      const notification = new Notification(title, { body, tag: task.id })
      notification.onclick = () => window.focus()
    }
  }, [tasks, permission])

  const askPermission = async () => {
    if (typeof Notification !== 'undefined') {
      setPermission(await Notification.requestPermission())
    }
  }

  return { permission, askPermission }
}
