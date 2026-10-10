import { CircleAlert, Eye, EyeOff } from 'lucide-react'
import { type FormEvent, useId, useState } from 'react'
import { Button } from './ui/Button'
import { IconButton } from './ui/IconButton'
import { fieldBox } from './ui/styles'

type SignInProps = {
  message: string | null
  onSubmit: (token: string) => void
}

export function SignIn({ message, onSubmit }: SignInProps) {
  const [value, setValue] = useState('')
  const [shown, setShown] = useState(false)
  // The daemon is asked on the next screen. If it says no, this screen comes back, fresh.
  const [connecting, setConnecting] = useState(false)
  const helpId = useId()
  const errorId = useId()

  const submit = (event: FormEvent) => {
    event.preventDefault()
    const token = value.trim()
    if (token !== '' && !connecting) {
      setConnecting(true)
      onSubmit(token)
    }
  }

  return (
    <main className="mx-auto flex min-h-screen max-w-md flex-col justify-center gap-4 px-4">
      <h1 className="text-xl font-semibold tracking-tight">Natamux</h1>
      <p className="text-muted">
        Paste the token the daemon printed when it started. It stays in this browser.
      </p>
      <form onSubmit={submit} className="flex flex-col gap-3">
        <div className="flex flex-col gap-1">
          <label htmlFor="daemon-token" className="text-xs font-medium text-ink-2">
            Daemon token
          </label>
          <div
            className={`${fieldBox} h-(--control-h-md) pr-1 ${message === null ? '' : 'border-danger-text!'}`}
          >
            <input
              id="daemon-token"
              type={shown ? 'text' : 'password'}
              value={value}
              onChange={(event) => setValue(event.target.value)}
              autoComplete="off"
              spellCheck={false}
              aria-invalid={message !== null}
              aria-describedby={message === null ? helpId : `${helpId} ${errorId}`}
              className="min-w-0 flex-1 bg-transparent font-mono text-sm text-ink outline-none"
            />
            <IconButton
              label={shown ? 'Hide the token' : 'Show the token'}
              icon={shown ? EyeOff : Eye}
              size="xs"
              aria-pressed={shown}
              onClick={() => setShown((open) => !open)}
            />
          </div>
          <p id={helpId} className="text-xs text-muted">
            It is <code className="font-mono">NATAMUX_TOKEN</code> of the daemon. Without it, the
            daemon makes one and prints it when it starts.
          </p>
        </div>
        {message !== null && (
          <p
            id={errorId}
            role="alert"
            className="flex items-start gap-1.5 text-xs font-medium text-danger-text"
          >
            <CircleAlert size={12} aria-hidden="true" className="mt-0.5 shrink-0" />
            {message}
          </p>
        )}
        <Button
          type="submit"
          variant="primary"
          size="md"
          busy={connecting}
          disabledReason={value.trim() === '' ? 'Paste the token first' : undefined}
        >
          Connect
        </Button>
      </form>
    </main>
  )
}
