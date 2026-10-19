import { describe, expect, it } from 'vitest'
import { claudeCodeAdapter, claudeCodeInputSchema } from '../claude-code-hooks.js'

function signalOf(name: string, extra: Record<string, unknown> = {}): string | undefined {
  return claudeCodeAdapter.normalize({ hook_event_name: name, ...extra })?.signal
}

describe('claudeCodeAdapter', () => {
  it.each([
    ['UserPromptSubmit', 'prompt'],
    ['PreToolUse', 'activity'],
    ['PostToolUse', 'activity'],
    ['PermissionRequest', 'asking'],
    ['Stop', 'turn-ended'],
    ['StopFailure', 'failed'],
  ])('turns %s into the signal %s', (name, signal) => {
    expect(signalOf(name)).toBe(signal)
  })

  it.each([
    ['permission_prompt', 'asking'],
    ['elicitation_dialog', 'asking'],
    ['agent_needs_input', 'asking'],
    ['idle_prompt', undefined],
    ['auth_success', undefined],
  ])('turns the notification %s into %s', (notificationType, signal) => {
    expect(signalOf('Notification', { notification_type: notificationType })).toBe(signal)
  })

  it('ignores a notification without a type', () => {
    expect(signalOf('Notification')).toBeUndefined()
  })

  it('ignores hooks it does not use', () => {
    expect(signalOf('SessionEnd')).toBeUndefined()
  })

  it('passes the working directory on', () => {
    expect(claudeCodeAdapter.normalize({ hook_event_name: 'Stop', cwd: '/work/T-0001' })?.cwd).toBe(
      '/work/T-0001',
    )
  })

  it('keeps the details of the hook and cuts a long message', () => {
    const hook = claudeCodeAdapter.normalize({
      hook_event_name: 'Notification',
      notification_type: 'permission_prompt',
      session_id: 's1',
      message: 'x'.repeat(900),
    })
    expect(hook?.details).toMatchObject({
      hook: 'Notification',
      notificationType: 'permission_prompt',
      sessionId: 's1',
    })
    expect(String(hook?.details.message)).toHaveLength(500)
  })

  describe('a turn end with background work', () => {
    const shell = (command: string, status = 'running') => ({
      id: 't1',
      type: 'shell',
      status,
      description: 'Run tests',
      command,
    })
    const stop = (extra: object) =>
      claudeCodeAdapter.normalize({ hook_event_name: 'Stop', ...extra })

    it('pauses the turn while a background shell runs, and says what it waits for', () => {
      const hook = stop({ background_tasks: [shell('pnpm  test')], session_crons: [] })
      expect(hook?.signal).toBe('turn-paused')
      expect(hook?.summary).toBe('Waiting for Bash: pnpm test')
      expect(hook?.details.waitingFor).toEqual(['Bash: pnpm test'])
    })

    it('names a shell by its command, else by its description', () => {
      const hook = stop({
        background_tasks: [
          { id: 'a', type: 'shell', status: 'running', command: 'pnpm build' },
          { id: 'b', type: 'shell', status: 'running', description: 'Watch CI' },
        ],
      })
      expect(hook?.details.waitingFor).toEqual(['Bash: pnpm build', 'Bash: Watch CI'])
    })

    it('names a task without a command by its description, or else by its type', () => {
      const hook = stop({
        background_tasks: [
          { id: 'm', type: 'monitor', status: 'running', description: 'watch CI' },
          { id: 's', type: 'subagent', status: 'running' },
        ],
      })
      expect(hook?.details.waitingFor).toEqual(['monitor: watch CI', 'subagent'])
      expect(hook?.summary).toBe('Waiting for monitor: watch CI and 1 more')
    })

    it.each(['subagent', 'monitor', 'workflow', 'teammate', 'cloud session', 'MCP task'])(
      'pauses the turn for a %s that runs on its own',
      (type) => {
        const hook = stop({ background_tasks: [{ id: 'x', type, status: 'running' }] })
        expect(hook?.signal).toBe('turn-paused')
        expect(hook?.details.waitingFor).toEqual([type])
      },
    )

    it('pauses the turn for a workflow and a background shell at the same time', () => {
      const hook = stop({
        background_tasks: [
          { id: 'w', type: 'workflow', status: 'running', description: 'implement' },
          shell('pnpm test'),
        ],
      })
      expect(hook?.signal).toBe('turn-paused')
      expect(hook?.details.waitingFor).toEqual(['workflow: implement', 'Bash: pnpm test'])
      expect(hook?.summary).toBe('Waiting for workflow: implement and 1 more')
    })

    it('ends the turn once the workflow is finished or stopped', () => {
      for (const status of ['completed', 'failed', 'killed', 'stopped']) {
        const hook = stop({ background_tasks: [{ id: 'w', type: 'workflow', status }] })
        expect(hook?.signal).toBe('turn-ended')
      }
    })

    it.each(['running', 'pending', 'queued', 'starting'])(
      'counts a task with the status %s as still running',
      (status) => {
        expect(stop({ background_tasks: [shell('ls', status)] })?.signal).toBe('turn-paused')
      },
    )

    it('counts a task without a status as still running', () => {
      expect(stop({ background_tasks: [{ id: 'x', type: 'shell', command: 'ls' }] })?.signal).toBe(
        'turn-paused',
      )
    })

    it('lists only the tasks that still run', () => {
      const hook = stop({ background_tasks: [shell('pnpm lint', 'completed'), shell('pnpm test')] })
      expect(hook?.signal).toBe('turn-paused')
      expect(hook?.details.waitingFor).toEqual(['Bash: pnpm test'])
    })

    it('ignores fields of a task it does not read', () => {
      const hook = stop({
        background_tasks: [{ ...shell('pnpm test'), pid: 42, startedAt: '2026-10-11T09:00:00Z' }],
      })
      expect(hook?.signal).toBe('turn-paused')
      expect(hook?.details.waitingFor).toEqual(['Bash: pnpm test'])
    })

    it('pauses the turn for a scheduled prompt of the session', () => {
      const hook = stop({
        background_tasks: [],
        session_crons: [{ id: 'c1', schedule: '*/5 * * * *', recurring: true, prompt: 'check CI' }],
      })
      expect(hook?.signal).toBe('turn-paused')
      expect(hook?.details.waitingFor).toEqual(['scheduled prompt: check CI'])
      expect(hook?.summary).toBe('Waiting for scheduled prompt: check CI')
    })

    it('pauses the turn for a one-shot prompt, with no background task list at all', () => {
      const hook = stop({ session_crons: [{ id: 'c1', recurring: false, prompt: 'check CI' }] })
      expect(hook?.signal).toBe('turn-paused')
    })

    it('names a scheduled prompt without its text by its schedule', () => {
      const hook = stop({ session_crons: [{ id: 'c1', schedule: '0 9 * * 1-5' }] })
      expect(hook?.details.waitingFor).toEqual(['scheduled prompt: 0 9 * * 1-5'])
    })

    it('lists background tasks and scheduled prompts together, at most five', () => {
      const hook = stop({
        background_tasks: [shell('a'), shell('b'), shell('c'), shell('d')],
        session_crons: [{ prompt: 'e' }, { prompt: 'f' }],
      })
      expect(hook?.details.waitingFor).toEqual([
        'Bash: a',
        'Bash: b',
        'Bash: c',
        'Bash: d',
        'scheduled prompt: e',
      ])
      expect(hook?.summary).toBe('Waiting for Bash: a and 5 more')
    })

    it.each(['completed', 'failed', 'killed', 'stopped'])(
      'ends the turn as before when the only task is %s',
      (status) => {
        const hook = stop({ background_tasks: [shell('ls', status)], session_crons: [] })
        expect(hook?.signal).toBe('turn-ended')
        expect(hook?.summary).toBeUndefined()
        expect(hook?.details.waitingFor).toBeUndefined()
      },
    )

    it.each([
      ['no fields', {}],
      ['empty lists', { background_tasks: [], session_crons: [] }],
      ['no task list and no crons', { session_crons: [] }],
      ['no tasks and no cron list', { background_tasks: [] }],
      ['entries without a type', { background_tasks: [{ status: 'running' }, null, 'x', 7] }],
      ['a type that is not text', { background_tasks: [{ type: 3, status: 'running' }] }],
      ['crons that are not objects', { session_crons: [null, 'daily', 5] }],
      ...[null, 'x', 42, {}].flatMap((value): [string, object][] => [
        [`a task list of ${JSON.stringify(value)}`, { background_tasks: value }],
        [`a cron list of ${JSON.stringify(value)}`, { session_crons: value }],
      ]),
    ])('ends the turn as before with %s', (_name, extra) => {
      const hook = stop(extra)
      expect(hook?.signal).toBe('turn-ended')
      expect(hook?.summary).toBeUndefined()
      expect(hook?.details.waitingFor).toBeUndefined()
    })

    it('skips a bad entry and keeps the good one', () => {
      const hook = stop({ background_tasks: [{ status: 'running' }, shell('pnpm test')] })
      expect(hook?.signal).toBe('turn-paused')
      expect(hook?.details.waitingFor).toEqual(['Bash: pnpm test'])
    })

    it('pauses the turn for a cron when the task list cannot be read', () => {
      const hook = stop({ background_tasks: 'x', session_crons: [{ prompt: 'check CI' }] })
      expect(hook?.signal).toBe('turn-paused')
    })

    it('keeps at most five entries of 200 characters, and a short summary', () => {
      const hook = stop({
        background_tasks: Array.from({ length: 7 }, () => shell('x'.repeat(300))),
      })
      const waitingFor = hook?.details.waitingFor as string[]
      expect(waitingFor).toHaveLength(5)
      for (const entry of waitingFor) {
        expect(entry).toHaveLength(200)
      }
      expect(hook?.summary?.endsWith(' and 6 more')).toBe(true)
      expect(hook?.summary).toHaveLength(80)
    })

    it('cuts a long description and a long scheduled prompt to 200 characters', () => {
      const hook = stop({
        background_tasks: [{ id: 'm', type: 'monitor', description: 'd'.repeat(300) }],
        session_crons: [{ prompt: 'p'.repeat(300) }],
      })
      const waitingFor = hook?.details.waitingFor as string[]
      expect(waitingFor.map((entry) => entry.length)).toEqual([200, 200])
      expect(waitingFor[1]?.startsWith('scheduled prompt: ppp')).toBe(true)
    })

    it('only looks for background work in a Stop', () => {
      expect(signalOf('PostToolUse', { background_tasks: [shell('ls')] })).toBe('activity')
    })
  })

  it('throws for input that is not a hook', () => {
    expect(() => claudeCodeAdapter.normalize({})).toThrow()
    expect(() => claudeCodeAdapter.normalize('Stop')).toThrow()
    expect(() => claudeCodeAdapter.normalize(undefined)).toThrow()
  })
})

describe('claudeCodeInputSchema', () => {
  it('needs an event name and drops the fields it does not read', () => {
    expect(claudeCodeInputSchema.safeParse({}).success).toBe(false)
    expect(
      claudeCodeInputSchema.parse({ hook_event_name: 'Stop', transcript_path: '/t.jsonl' }),
    ).toEqual({ hook_event_name: 'Stop' })
  })
})

describe('the summary of a Claude Code hook', () => {
  it('is given for the tool about to run, and for a new prompt', () => {
    const before = claudeCodeAdapter.normalize({
      hook_event_name: 'PreToolUse',
      tool_name: 'Bash',
      tool_input: { command: 'ls' },
    })
    expect(before?.summary).toBe('Bash: ls')
    expect(
      claudeCodeAdapter.normalize({ hook_event_name: 'PostToolUse', tool_name: 'Bash' })?.summary,
    ).toBeUndefined()
    expect(claudeCodeAdapter.normalize({ hook_event_name: 'UserPromptSubmit' })?.summary).toBe(
      'Reading the prompt',
    )
  })
})
