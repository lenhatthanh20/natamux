import type { Task } from '@natamux/core'
import { useQueryClient } from '@tanstack/react-query'
import { useEffect, useRef, useState } from 'react'
import {
  applyLiveMessages,
  closeSocket,
  type LiveMessage,
  liveSocketUrl,
  parseLiveMessage,
} from '../lib/live'
import { createTaskBatch } from '../lib/task-batch'

export type LiveStatus = 'connecting' | 'live' | 'reconnecting'

const MAX_RETRY_DELAY_MS = 15_000

// Keeps one WebSocket open to the daemon and calls onTask for every changed task, onDeleted
// when a task is removed, and onRepos when a project was added or removed. Activity, shells and
// workspace changes go straight into the query cache, once per frame.
export function useLiveTasks(
  token: string,
  onTask: (task: Task) => void,
  onRepos?: () => void,
  onDeleted?: (taskId: string) => void,
): LiveStatus {
  const queryClient = useQueryClient()
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
    let opened = false
    const batch = createTaskBatch<LiveMessage>((messages) =>
      applyLiveMessages(queryClient, messages),
    )

    const connect = () => {
      socket = new WebSocket(liveSocketUrl(window.location, token))
      socket.onopen = () => {
        retries = 0
        setStatus('live')
        // Messages sent while the socket was down are lost, so what they would have changed is
        // read again: everything after a reconnect. The first connect comes right after the first
        // reads, so only the task and project lists, as they change most.
        if (opened) {
          void queryClient.invalidateQueries()
        } else {
          void queryClient.invalidateQueries({ queryKey: ['tasks'] })
          void queryClient.invalidateQueries({ queryKey: ['repos'] })
        }
        opened = true
      }
      socket.onmessage = (event) => {
        const message = typeof event.data === 'string' ? parseLiveMessage(event.data) : undefined
        if (message === undefined) {
          return
        }
        if (message.kind === 'task') {
          onTaskRef.current(message.task)
          if (message.logged) {
            batch.push(message)
          }
        } else if (message.kind === 'task.deleted') {
          onDeletedRef.current?.(message.taskId)
        } else if (message.kind === 'repos.changed') {
          onReposRef.current?.()
        } else {
          batch.push(message)
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
      batch.cancel()
      if (socket !== undefined) {
        closeSocket(socket)
      }
    }
  }, [token, queryClient])

  return status
}
