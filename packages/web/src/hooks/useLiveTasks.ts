import type { Task } from '@natamux/core'
import { useEffect, useRef, useState } from 'react'
import { closeSocket, liveSocketUrl, parseLiveMessage } from '../lib/live'

export type LiveStatus = 'connecting' | 'live' | 'reconnecting'

const MAX_RETRY_DELAY_MS = 15_000

// Keeps one WebSocket open to the daemon and calls onTask for every changed task.
export function useLiveTasks(token: string, onTask: (task: Task) => void): LiveStatus {
  const [status, setStatus] = useState<LiveStatus>('connecting')
  const onTaskRef = useRef(onTask)

  useEffect(() => {
    onTaskRef.current = onTask
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
        const task = typeof event.data === 'string' ? parseLiveMessage(event.data) : undefined
        if (task) {
          onTaskRef.current(task)
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
