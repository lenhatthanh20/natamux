import { type FormEvent, useState } from 'react'

type AgentCommentProps = {
  path: string
  // The line the cursor is on in the diff, when there is one.
  line?: number | null
  send: (text: string) => Promise<unknown>
}

// The text the agent gets: the file (and line) first, so the agent knows where to look.
export function commentForAgent(path: string, comment: string, line?: number | null): string {
  const where = line ? `${path}, line ${line}` : path
  return `Review comment on ${where}:\n${comment.trim()}`
}

// A review comment on the file shown, pasted into the agent's terminal as its next prompt.
export function AgentComment({ path, line, send }: AgentCommentProps) {
  const [comment, setComment] = useState('')
  const [status, setStatus] = useState<string | null>(null)
  const [sending, setSending] = useState(false)

  const submit = async (event: FormEvent) => {
    event.preventDefault()
    if (comment.trim() === '' || sending) {
      return
    }
    setSending(true)
    setStatus(null)
    try {
      await send(commentForAgent(path, comment, line))
      setComment('')
      setStatus('Sent to the agent.')
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error))
    } finally {
      setSending(false)
    }
  }

  return (
    <form onSubmit={submit} className="mt-3 flex flex-col gap-2">
      <label htmlFor="agent-comment" className="text-muted">
        Comment on {path}
        {line ? `, line ${line},` : ''} for the agent
      </label>
      <textarea
        id="agent-comment"
        rows={3}
        value={comment}
        onChange={(event) => setComment(event.target.value)}
        placeholder="What should change, and why"
        className="rounded-md border border-line bg-surface px-3 py-2"
      />
      <div className="flex items-center gap-3">
        <button
          type="submit"
          disabled={sending || comment.trim() === ''}
          className="rounded-md border border-line px-3 py-1.5 font-medium hover:border-accent disabled:opacity-50"
        >
          Send to agent
        </button>
        {status !== null && <span role="status">{status}</span>}
      </div>
    </form>
  )
}
