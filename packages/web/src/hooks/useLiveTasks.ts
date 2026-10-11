import type { Task } from '@natamux/core'
import { useEffect, useRef, useState } from 'react'
import {
  closeSocket,
  deletedTaskId,
  isReposChanged,
  liveSocketUrl,
  parseLiveMessage,
} from '../lib/live'

export type LiveStatus = 'connecting' | 'live' | 'reconnecting'

const MAX_RETRY_DELAY_MS = 15_000

// Keeps one WebSocket open to the daemon and calls onTask for every changed task, onDeleted
// when a task is removed, and onRepos when a project was added or removed.
export function useLiveTasks(
  token: string,
  onTask: (task: Task) => void,
  onRepos?: () => void,
  onDeleted?: (taskId: string) => void,
): LiveStatus {
  const [status, setStatus] = useState<LiveStatus>('connecting')
  const onTaskRef = useRef(onTask)
  const onReposRef = useRef(onRepos)
  const onDeletedRef = useRef(onDeleted)

  useEffect(() => {
    onTaskRef.current = onTask
    onReposRef.current = onRepos
    onDeletedRef.current = onDeleted
  })

  useEffect(() => {
    let socket: WebSocket | undefined
    let retries = 0
    let timer: ReturnType<typeof setTimeout> | undefined
    let stopped = false

    const connect = () => {
      socket = new WebSocket(liveSocketUrl(window.location, token))
      socket.onopen = () => {
        retries = 0
        setStatus('live')
      }
      socket.onmessage = (event) => {
        if (typeof event.data !== 'string') {
          return
        }
        const deleted = deletedTaskId(event.data)
        if (deleted !== undefined) {
          onDeletedRef.current?.(deleted)
          return
        }
        const task = parseLiveMessage(event.data)
        if (task) {
          onTaskRef.current(task)
        } else if (isReposChanged(event.data)) {
          onReposRef.current?.()
        }
      }
      socket.onclose = () => {
        if (stopped) {
          return
        }
        setStatus('reconnecting')
        timer = setTimeout(connect, Math.min(1000 * 2 ** retries, MAX_RETRY_DELAY_MS))
        retries += 1
      }
    }

    connect()

    return () => {
      stopped = true
      clearTimeout(timer)
      if (socket !== undefined) {
        closeSocket(socket)
      }
    }
  }, [token])

  return status
}
