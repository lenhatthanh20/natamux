import type { ReactNode } from 'react'

export type SkipChoice = {
  on: boolean
  busy: boolean
  onChange: (on: boolean) => void
}

export type WorkflowChoice = {
  on: boolean
  // The agent of the task can run a workflow.
  allowed: boolean
  busy: boolean
  onChange: (on: boolean) => void
}

// The punctuation that ends a sentence or closes a bracket is not part of the address.
const ADDRESS = /https?:\/\/\S*[^\s.,;:!?)\]}'"]/g
const RUN_ADDRESS = /\/actions\/runs\/\d+/

// A reason with its addresses as links. A CI run address is a short "view run", so the reason
// stays on one line.
export function ReasonText({ text }: { text: string }) {
  const parts: ReactNode[] = []
  let from = 0
  for (const match of text.matchAll(ADDRESS)) {
    const [address] = match
    const at = match.index ?? 0
    parts.push(
      text.slice(from, at),
      <a
        key={at}
        href={address}
        target="_blank"
        rel="noopener noreferrer"
        data-tip={address}
        className="text-link hover:underline"
      >
        {RUN_ADDRESS.test(address) ? 'view run' : address}
      </a>,
    )
    from = at + address.length
  }
  parts.push(text.slice(from))
  return <>{parts}</>
}

// Once the stage started, a hint is a tip on its checkbox, so the running card stays short.
function BoxHint({ hint, started }: { hint: string; started: boolean }) {
  return started ? null : <span className="pl-5.5 text-xs text-muted">{hint}</span>
}

export function WorkflowBox({ choice, started }: { choice: WorkflowChoice; started: boolean }) {
  const hint = !choice.allowed
    ? 'Only Claude Code can run a workflow.'
    : started
      ? 'More accurate, uses many more tokens. Takes effect the next time this stage starts.'
      : 'More accurate, uses many more tokens.'
  return (
    <div className="mt-2 flex flex-col gap-0.5">
      <label
        className="flex w-fit items-center gap-2 text-sm text-ink-2"
        data-tip={started && choice.allowed ? hint : undefined}
      >
        <input
          type="checkbox"
          checked={choice.on}
          // An agent that cannot run one may still turn off a choice made for an earlier agent.
          disabled={(!choice.allowed && !choice.on) || choice.busy}
          onChange={(event) => choice.onChange(event.target.checked)}
          className="size-3.5 accent-accent"
        />
        Run with a workflow
      </label>
      {/* The Claude Code hint stays in view: it says why the box is off. */}
      <BoxHint hint={hint} started={started && choice.allowed} />
    </div>
  )
}

export type CiChoice = {
  on: boolean
  busy: boolean
  onChange: (on: boolean) => void
}

const CI_HINT =
  'When the agent is done, its work is pushed to a natamux-ci branch, and the stage passes once CI passes. A failure goes back to the agent.'

export function CiBox({ choice, started }: { choice: CiChoice; started: boolean }) {
  return (
    <div className="mt-2 flex flex-col gap-0.5">
      <label
        className="flex w-fit items-center gap-2 text-sm text-ink-2"
        data-tip={started ? CI_HINT : undefined}
      >
        <input
          type="checkbox"
          checked={choice.on}
          disabled={choice.busy}
          onChange={(event) => choice.onChange(event.target.checked)}
          className="size-3.5 accent-accent"
        />
        Check on CI
      </label>
      <BoxHint hint={CI_HINT} started={started} />
    </div>
  )
}
