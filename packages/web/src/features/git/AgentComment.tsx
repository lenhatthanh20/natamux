import type { NewReviewComment } from '@natamux/core'
import { X } from 'lucide-react'
import { type FormEvent, type KeyboardEvent, useLayoutEffect, useRef, useState } from 'react'
import { Button } from '../../components/ui/Button'
import { IconButton } from '../../components/ui/IconButton'
import { textarea } from '../../components/ui/styles'
import { type CodeSelection, linesLabel } from '../../lib/changes'

type AgentCommentProps = {
  path: string
  // The line the cursor is on in the diff, when there is one.
  line?: number | null
  // Lines picked in the diff. They replace the cursor line.
  quote?: CodeSelection | null
  onClearQuote?: () => void
  // One-off prompt, pasted into the agent's terminal now ("Send now").
  send: (text: string) => Promise<unknown>
  // Keeps the comment as a draft for the next batch ("Add comment"). Without it, only Send now.
  add?: (input: NewReviewComment) => Promise<unknown>
}

// The message route takes 20,000 characters. A longer quote would leave no room for the comment.
export const QUOTE_MAX = 10_000

// A fence longer than any run of backticks in the code, so the code cannot end the block.
function fenced(text: string): string {
  const longest = Math.max(0, ...(text.match(/`+/g) ?? []).map((run) => run.length))
  const fence = '`'.repeat(Math.max(3, longest + 1))
  return `${fence}\n${text}\n${fence}`
}

// The text the agent gets: the file (and line) first, so the agent knows where to look.
export function commentForAgent(
  path: string,
  comment: string,
  line?: number | null,
  quote?: CodeSelection | null,
): string {
  if (!quote) {
    const where = line ? `${path}, line ${line}` : path
    return `Review comment on ${where}:\n${comment.trim()}`
  }
  const old = quote.side === 'old'
  const code =
    quote.text.length > QUOTE_MAX
      ? `(Too long to quote. Read these lines in ${old ? 'the last commit' : 'the file'}.)`
      : fenced(quote.text)
  const where = `${path}, ${linesLabel(quote, '-')}${old ? ' before the change' : ''}`
  return `Review comment on ${where}:\n${code}\n${comment.trim()}`
}

// "lines 12–14 (before the change)", for the label and the chip.
function quoteLabel(quote: CodeSelection): string {
  return `${linesLabel(quote)}${quote.side === 'old' ? ' (before the change)' : ''}`
}

function labelFor(line: number | null | undefined, quote: CodeSelection | null | undefined) {
  if (quote) {
    return `Comment on ${quoteLabel(quote)} for the agent`
  }
  return line ? `Comment on line ${line} for the agent` : 'Comment on this file for the agent'
}

// Four lines, then the field scrolls.
const MAX_FIELD_HEIGHT = 96

// A review comment on the file shown, pasted into the agent's terminal as its next prompt. It sits
// at the bottom of the diff column: one line that grows as the user types.
export function AgentComment({ path, line, quote, onClearQuote, send, add }: AgentCommentProps) {
  const [comment, setComment] = useState('')
  const [status, setStatus] = useState<string | null>(null)
  const [sending, setSending] = useState(false)
  const field = useRef<HTMLTextAreaElement>(null)

  // biome-ignore lint/correctness/useExhaustiveDependencies: the text decides the height.
  useLayoutEffect(() => {
    const element = field.current
    if (element) {
      element.style.height = 'auto'
      element.style.height = `${Math.min(element.scrollHeight, MAX_FIELD_HEIGHT)}px`
    }
  }, [comment])

  // The call, its words on success, and what to do after. A failure keeps the text and the quote.
  const act = async (call: () => Promise<unknown>, done: string) => {
    if (comment.trim() === '' || sending) {
      return
    }
    setSending(true)
    setStatus(null)
    try {
      await call()
      setComment('')
      onClearQuote?.()
      setStatus(done)
    } catch (error) {
      setStatus(error instanceof Error ? error.message : String(error))
    } finally {
      setSending(false)
    }
  }
  const sendNow = () =>
    act(() => send(commentForAgent(path, comment, line, quote)), 'Sent to the agent.')
  // The quote's lines and side; else the cursor line of the new text; else the whole file.
  const addDraft = () => {
    const target = quote
      ? { side: quote.side, start: quote.start, end: quote.end }
      : { side: 'new' as const, start: line ?? 0, end: line ?? 0 }
    const call = () => add?.({ path, ...target, body: comment.trim() }) ?? Promise.resolve()
    return act(call, 'Comment added.')
  }
  const submit = add === undefined ? sendNow : addDraft

  const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (event.key === 'Enter' && (event.metaKey || event.ctrlKey) && !event.shiftKey) {
      event.preventDefault()
      void submit()
    }
  }

  return (
    <form
      onSubmit={(event: FormEvent) => {
        event.preventDefault()
        void submit()
      }}
      className="flex shrink-0 flex-col gap-1 border-t border-line bg-surface px-3 py-2"
    >
      <div className="flex items-baseline gap-2">
        <label htmlFor="agent-comment" className="text-xs font-medium text-ink-2">
          {labelFor(line, quote)}
        </label>
        {status !== null && (
          <span role="status" className="min-w-0 truncate text-xs text-muted">
            {status}
          </span>
        )}
      </div>
      {quote && (
        <div className="flex min-w-0 items-center gap-1 self-start rounded-md border border-line bg-raised py-0.5 pr-0.5 pl-2 text-xs text-ink-2">
          <span className="min-w-0 truncate">
            {quoteLabel(quote).replace(/^l/, 'L')}
            {quote.text.length > QUOTE_MAX &&
              ' · too long to quote, only the line numbers are sent'}
          </span>
          <IconButton size="xs" icon={X} label="Remove the selected lines" onClick={onClearQuote} />
        </div>
      )}
      <div className="flex items-end gap-2">
        <textarea
          ref={field}
          id="agent-comment"
          rows={1}
          value={comment}
          onChange={(event) => setComment(event.target.value)}
          onKeyDown={onKeyDown}
          placeholder={
            quote
              ? 'Ask about these lines, or say what should change'
              : 'What should change, and why'
          }
          className={`${textarea} min-h-(--control-h) flex-1 resize-none py-1`}
        />
        {add !== undefined && (
          <Button
            variant="ghost"
            busy={sending}
            disabledReason={comment.trim() === '' ? 'Write a comment first' : undefined}
            data-tip="Paste this comment into the agent's terminal now"
            onClick={() => void sendNow()}
          >
            Send now
          </Button>
        )}
        <Button
          type="submit"
          busy={sending}
          disabledReason={comment.trim() === '' ? 'Write a comment first' : undefined}
          shortcut="Mod+Enter"
        >
          {add === undefined ? 'Send now' : 'Add comment'}
        </Button>
      </div>
    </form>
  )
}
