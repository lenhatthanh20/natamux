import { type FormEvent, useState } from 'react'

type SignInProps = {
  message: string | null
  onSubmit: (token: string) => void
}

export function SignIn({ message, onSubmit }: SignInProps) {
  const [value, setValue] = useState('')

  const submit = (event: FormEvent) => {
    event.preventDefault()
    const token = value.trim()
    if (token !== '') {
      onSubmit(token)
    }
  }

  return (
    <main className="mx-auto flex min-h-screen max-w-md flex-col justify-center gap-4 px-4">
      <h1 className="text-xl font-semibold">Helmsman</h1>
      <p className="text-muted">
        Paste the token the daemon printed when it started. It stays in this browser.
      </p>
      <form onSubmit={submit} className="flex flex-col gap-3">
        <label className="flex flex-col gap-1">
          <span className="font-medium">Daemon token</span>
          <input
            type="password"
            value={value}
            onChange={(event) => setValue(event.target.value)}
            autoComplete="off"
            className="rounded-md border border-line bg-surface px-3 py-2"
          />
        </label>
        {message !== null && (
          <p role="alert" className="text-sm font-medium text-[#a02318] dark:text-[#ff9d92]">
            {message}
          </p>
        )}
        <button
          type="submit"
          className="rounded-md bg-ink px-3 py-2 font-medium text-canvas hover:opacity-90"
        >
          Connect
        </button>
      </form>
    </main>
  )
}
