import {
  appendFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  symlinkSync,
  truncateSync,
  writeFileSync,
} from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { type DaemonMessage, EventBus } from '../../shared/event-bus.js'
import { sessionFile, UsageService, usagePath } from '../usage-service.js'

// One message of a Claude Code session.
function claudeLine(id: string, input: number, output: number, model = 'claude-opus-5-5'): string {
  const usage = {
    input_tokens: input,
    cache_creation_input_tokens: 0,
    cache_read_input_tokens: 0,
    output_tokens: output,
  }
  return `${JSON.stringify({ type: 'assistant', message: { id, model, usage } })}\n`
}

describe('UsageService', () => {
  let root: string
  let home: string
  let dataDir: string
  let bus: EventBus
  let sent: DaemonMessage[]
  let service: UsageService
  let session: string

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'natamux-usage-'))
    home = join(root, 'home')
    dataDir = join(root, 'data')
    mkdirSync(join(home, '.claude'), { recursive: true })
    bus = new EventBus()
    sent = []
    bus.subscribe((message) => sent.push(message))
    service = new UsageService({ dataDir, bus, home })
    session = join(home, '.claude', 'session.jsonl')
  })

  afterEach(() => {
    rmSync(root, { recursive: true, force: true })
  })

  const read = async (stage: string | null, agent = 'claude-code', path = session) => {
    service.seen('T-0001', stage, agent, path)
    await service.settle()
  }

  it('adds a session to the stage of its turn, and sends the new usage', async () => {
    writeFileSync(session, claudeLine('m1', 1000, 1000))

    await read('implement')

    const usage = service.usageOf('T-0001')
    expect(usage).toEqual({
      tokens: 2000,
      costUsd: expect.closeTo(0.024),
      unpricedModels: [],
      stages: [
        {
          stage: 'implement',
          agent: 'claude-code',
          model: 'claude-opus-5-5',
          input: 1000,
          cacheRead: 0,
          cacheWrite: 0,
          output: 1000,
          costUsd: expect.closeTo(0.024),
        },
      ],
    })
    expect(sent).toEqual([{ kind: 'usage.changed', taskId: 'T-0001', usage }])
    expect(service.all()).toEqual({ 'T-0001': usage })
  })

  it('gives the next stage only what came after the last read', async () => {
    writeFileSync(session, claudeLine('m1', 100, 10))
    await read('implement')
    appendFileSync(session, claudeLine('m2', 50, 5))

    await read('review')
    await read('review')

    expect(service.usageOf('T-0001')?.stages.map((row) => [row.stage, row.input])).toEqual([
      ['implement', 100],
      ['review', 50],
    ])
    expect(sent).toHaveLength(2)
  })

  it('puts a turn on no stage under ""', async () => {
    writeFileSync(session, claudeLine('m1', 100, 10))

    await read(null)

    expect(service.usageOf('T-0001')?.stages[0]?.stage).toBe('')
  })

  it('reads a file again after a read that ran when its hook came', async () => {
    writeFileSync(session, claudeLine('m1', 100, 10))
    service.seen('T-0001', 'implement', 'claude-code', session)
    appendFileSync(session, claudeLine('m2', 50, 5))
    service.seen('T-0001', 'implement', 'claude-code', session)
    await service.settle()

    expect(service.usageOf('T-0001')?.stages[0]?.input).toBe(150)
  })

  it('lists a model with no price, and leaves the cost of its rows null', async () => {
    writeFileSync(session, claudeLine('m1', 100, 10, 'mystery-model'))

    await read('implement')

    expect(service.usageOf('T-0001')).toMatchObject({
      tokens: 110,
      costUsd: null,
      unpricedModels: ['mystery-model'],
      stages: [{ model: 'mystery-model', costUsd: null }],
    })
  })

  it('reads nothing for an agent that writes no usage', async () => {
    writeFileSync(session, claudeLine('m1', 100, 10))

    await read('implement', 'cursor')

    expect(service.usageOf('T-0001')).toBeNull()
  })

  it('keeps the usage over a restart, and forgets it when the task is deleted', async () => {
    writeFileSync(session, claudeLine('m1', 100, 10))
    await read('implement')

    const again = new UsageService({ dataDir, bus, home })
    expect(again.usageOf('T-0001')).toEqual(service.usageOf('T-0001'))

    bus.publish({ kind: 'task.deleted', taskId: 'T-0001' })
    expect(again.usageOf('T-0001')).toBeNull()
    expect(again.all()).toEqual({})
  })

  it('reads a broken usage file as no usage', () => {
    mkdirSync(join(dataDir, 'tasks'), { recursive: true })
    writeFileSync(usagePath(dataDir, 'T-0002'), '{"files":')

    expect(new UsageService({ dataDir, home }).usageOf('T-0002')).toBeNull()
  })

  it('writes the usage file', async () => {
    writeFileSync(session, claudeLine('m1', 100, 10))
    await read('implement')

    const stored = JSON.parse(readFileSync(usagePath(dataDir, 'T-0001'), 'utf8'))
    expect(stored.files[session].agent).toBe('claude-code')
    expect(stored.stages.implement['claude-code']['claude-opus-5-5'].input).toBe(100)
    expect(existsSync(`${usagePath(dataDir, 'T-0001')}.tmp`)).toBe(false)
  })

  describe('sessionFile', () => {
    it('takes a session file in the home folder', async () => {
      writeFileSync(session, '')
      expect(await sessionFile(session, home)).not.toBeNull()
    })

    it('refuses a path outside the home folder, or not to a session file', async () => {
      const outside = join(root, 'outside.jsonl')
      writeFileSync(outside, '')
      const text = join(home, 'notes.txt')
      writeFileSync(text, '')
      const folder = join(home, 'folder.jsonl')
      mkdirSync(folder)

      expect(await sessionFile(outside, home)).toBeNull()
      expect(await sessionFile(text, home)).toBeNull()
      expect(await sessionFile(folder, home)).toBeNull()
      expect(await sessionFile('.claude/session.jsonl', home)).toBeNull()
      expect(await sessionFile(join(home, 'missing.jsonl'), home)).toBeNull()
    })

    // Windows makes links only with more rights.
    it.skipIf(process.platform === 'win32')('refuses a link to a file outside', async () => {
      const outside = join(root, 'outside.jsonl')
      writeFileSync(outside, '')
      const link = join(home, 'link.jsonl')
      symlinkSync(outside, link)

      expect(await sessionFile(link, home)).toBeNull()
    })

    it('refuses a file over 200 MB', async () => {
      writeFileSync(session, '')
      truncateSync(session, 200 * 1024 * 1024 + 1)

      expect(await sessionFile(session, home)).toBeNull()
    })

    it('is not read when refused', async () => {
      const outside = join(root, 'outside.jsonl')
      writeFileSync(outside, claudeLine('m1', 100, 10))

      await read('implement', 'claude-code', outside)

      expect(service.usageOf('T-0001')).toBeNull()
    })
  })
})
