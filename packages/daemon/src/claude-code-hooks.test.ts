import { describe, expect, it } from 'vitest'
import { claudeCodeAdapter, claudeCodeInputSchema, toolSummary } from './claude-code-hooks.js'

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

describe('toolSummary', () => {
  const summary = (tool: string, toolInput: Record<string, unknown>) =>
    toolSummary({
      hook_event_name: 'PreToolUse',
      cwd: '/w',
      tool_name: tool,
      tool_input: toolInput,
    })

  it.each([
    ['Edit', { file_path: '/w/src/a.ts' }, 'Edit src/a.ts'],
    ['Read', { file_path: '/other/b.ts' }, 'Read /other/b.ts'],
    ['Bash', { command: 'pnpm  test\n--run' }, 'Bash: pnpm test --run'],
    ['Grep', { pattern: 'TODO' }, 'Grep TODO'],
    ['TodoWrite', {}, 'TodoWrite'],
  ])('says %s %j as %j', (tool, toolInput, text) => {
    expect(summary(tool, toolInput)).toBe(text)
  })

  it('cuts a long line', () => {
    expect(summary('Bash', { command: 'x'.repeat(200) })).toHaveLength(80)
  })

  it('has nothing to say without a tool', () => {
    expect(toolSummary({ hook_event_name: 'Stop' })).toBeUndefined()
  })

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
