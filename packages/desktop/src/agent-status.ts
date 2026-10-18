// What the dashboard reports for the app icon and the awake hold.
export type AgentStatus = { needsYou: number; running: number }

const isCount = (value: unknown): value is number =>
  Number.isSafeInteger(value) && (value as number) >= 0

// The page sends this, so only two whole counts of zero or more are taken.
export function readAgentStatus(value: unknown): AgentStatus | undefined {
  if (typeof value !== 'object' || value === null) {
    return undefined
  }
  const { needsYou, running } = value as Record<string, unknown>
  return isCount(needsYou) && isCount(running) ? { needsYou, running } : undefined
}

// Electron's powerSaveBlocker, or a fake in tests.
type Blocker = {
  start(type: 'prevent-app-suspension'): number
  stop(id: number): void
}

// Keeps the computer from sleeping while agents run. The display can still sleep.
export class AwakeGuard {
  private readonly blocker: Blocker
  private id: number | undefined

  constructor(blocker: Blocker) {
    this.blocker = blocker
  }

  update(hold: boolean): void {
    if (hold && this.id === undefined) {
      this.id = this.blocker.start('prevent-app-suspension')
    } else if (!hold && this.id !== undefined) {
      this.blocker.stop(this.id)
      this.id = undefined
    }
  }
}
