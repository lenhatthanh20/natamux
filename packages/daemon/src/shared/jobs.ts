import type { EventBus, Job } from './event-bus.js'

// The jobs that run now. In memory only: they end with the daemon.
export class Jobs {
  private readonly jobs = new Map<string, Job>()
  private next = 1

  constructor(
    private readonly bus?: EventBus,
    private readonly now: () => number = Date.now,
  ) {}

  async run<T>(job: Omit<Job, 'id' | 'startedAt'>, work: () => Promise<T>): Promise<T> {
    const id = `job-${this.next++}`
    this.jobs.set(id, { id, ...job, startedAt: new Date(this.now()).toISOString() })
    this.publish()
    try {
      return await work()
    } finally {
      this.jobs.delete(id)
      this.publish()
    }
  }

  // Oldest first: a Map keeps the order of insertion.
  list(): Job[] {
    return [...this.jobs.values()]
  }

  private publish(): void {
    this.bus?.publish({ kind: 'jobs.changed', jobs: this.list() })
  }
}
