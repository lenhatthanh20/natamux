import type { Task, TaskState } from '@natamux/core'
import { useEffect, useRef, useState } from 'react'
import { useStableCallback } from '../../hooks/useStableCallback'
import {
  notificationFor,
  statesById,
  tasksThatFinished,
  tasksThatStartedWaiting,
} from '../../lib/notify'

export type NotifyPermission = NotificationPermission | 'unsupported'

function currentPermission(): NotifyPermission {
  return typeof Notification === 'undefined' ? 'unsupported' : Notification.permission
}

// Shows a system notification when a task starts to wait, or is done, while the dashboard is not
// in front. A click on one opens its task.
export function useTaskNotifications(tasks: readonly Task[], onOpen: (taskId: string) => void) {
  const [permission, setPermission] = useState<NotifyPermission>(currentPermission)
  const open = useStableCallback(onOpen)
  const seen = useRef<Map<string, TaskState> | null>(null)

  useEffect(() => {
    const previous = seen.current
    seen.current = statesById(tasks)
    if (previous === null || permission !== 'granted' || !document.hidden) {
      return
    }
    const waiting = tasksThatStartedWaiting(previous, tasks)
    for (const task of [...waiting, ...tasksThatFinished(previous, tasks)]) {
      const { title, body } = notificationFor(task)
      const notification = new Notification(title, { body, tag: task.id })
      notification.onclick = () => {
        window.focus()
        open(task.id)
        notification.close()
      }
    }
  }, [tasks, permission, open])

  const askPermission = async () => {
    if (typeof Notification !== 'undefined') {
      setPermission(await Notification.requestPermission())
    }
  }

  return { permission, askPermission }
}
