import { describe, expect, it } from 'vitest'
import { type DaemonMessage, EventBus } from '../event-bus.js'
import { Jobs } from '../jobs.js'

function setup() {
  const bus = new EventBus()
  const messages: DaemonMessage[] = []
  bus.subscribe((message) => messages.push(message))
  let now = Date.parse('2026-10-21T09:00:00.000Z')
  const jobs = new Jobs(bus, () => now++)
  return { jobs, messages }
}

describe('Jobs', () => {
  it('lists a job while its work runs and drops it after', async () => {
    const { jobs, messages } = setup()
    let finish: () => void = () => undefined
    const work = new Promise<string>((resolve) => {
      finish = () => resolve('done')
    })
    const running = jobs.run({ kind: 'push', label: 'Pushing', taskId: 'T-0001' }, () => work)

    expect(jobs.list()).toEqual([
      {
        id: expect.any(String),
        kind: 'push',
        label: 'Pushing',
        taskId: 'T-0001',
        startedAt: '2026-10-21T09:00:00.000Z',
      },
    ])
    expect(messages).toEqual([{ kind: 'jobs.changed', jobs: jobs.list() }])

    finish()
    await expect(running).resolves.toBe('done')
    expect(jobs.list()).toEqual([])
    expect(messages.at(-1)).toEqual({ kind: 'jobs.changed', jobs: [] })
    expect(messages).toHaveLength(2)
  })

  it('drops a job whose work throws, and passes the error on', async () => {
    const { jobs, messages } = setup()
    const running = jobs.run({ kind: 'push', label: 'Pushing', taskId: 'T-0001' }, async () => {
      throw new Error('rejected')
    })
    await expect(running).rejects.toThrow('rejected')
    expect(jobs.list()).toEqual([])
    expect(messages.at(-1)).toEqual({ kind: 'jobs.changed', jobs: [] })
  })

  it('lists jobs oldest first, each with its own id', async () => {
    const { jobs } = setup()
    const never = () => new Promise<never>(() => undefined)
    void jobs.run({ kind: 'push', label: 'Pushing', taskId: 'T-0001' }, never)
    void jobs.run({ kind: 'import', label: 'Importing issues', taskId: null }, never)
    const list = jobs.list()
    expect(list.map((job) => job.label)).toEqual(['Pushing', 'Importing issues'])
    expect(list[0]?.id).not.toBe(list[1]?.id)
  })
})
