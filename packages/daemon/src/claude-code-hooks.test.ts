import { describe, expect, it } from 'vitest'
import { claudeCodeAdapter, claudeCodeInputSchema } from './claude-code-hooks.js'

function signalOf(name: string, extra: Record<string, unknown> = {}): string | undefined {
  return claudeCodeAdapter.normalize({ hook_event_name: name, ...extra })?.signal
}

describe('claudeCodeAdapter', () => {
  it.each([
    ['UserPromptSubmit', 'prompt'],
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
    expect(signalOf('PreToolUse')).toBeUndefined()
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
      claudeCodeInputSchema.parse({ hook_event_name: 'Stop', tool_input: { command: 'ls' } }),
    ).toEqual({ hook_event_name: 'Stop' })
  })
})
