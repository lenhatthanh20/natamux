import { describe, expect, it } from 'vitest'
import { findAdapter, knownAgentIds } from './agent-adapters.js'
import { COPILOT_EVENT_FIELD } from './copilot-hooks.js'

const normalize = (agent: string, raw: unknown) => findAdapter(agent)?.normalize(raw)

describe('the adapters', () => {
  it('cover every agent Helmsman starts', () => {
    expect(knownAgentIds()).toEqual(['claude-code', 'codex', 'gemini', 'copilot', 'cursor'])
  })
})

describe('Codex', () => {
  it.each([
    ['UserPromptSubmit', 'prompt'],
    ['PreToolUse', 'activity'],
    ['PostToolUse', 'activity'],
    ['PermissionRequest', 'asking'],
    ['Stop', 'turn-ended'],
    ['Interrupt', 'turn-ended'],
  ])('%s is %s', (event, signal) => {
    expect(normalize('codex', { hook_event_name: event, cwd: '/w' })?.signal).toBe(signal)
  })

  it('ignores the events Helmsman does not use, and shows the shell command it runs', () => {
    expect(normalize('codex', { hook_event_name: 'SessionStart' })).toBeUndefined()
    const hook = normalize('codex', {
      hook_event_name: 'PreToolUse',
      cwd: '/w',
      session_id: 's1',
      tool_name: 'shell',
      tool_input: { command: ['bash', '-lc', 'pnpm test'] },
    })
    expect(hook).toMatchObject({ cwd: '/w', summary: 'shell: pnpm test' })
    expect(hook?.details).toEqual({ hook: 'PreToolUse', sessionId: 's1' })
  })
})

describe('Gemini CLI', () => {
  it.each([
    [{ hook_event_name: 'BeforeAgent' }, 'prompt'],
    [{ hook_event_name: 'BeforeTool', tool_name: 'read_file' }, 'activity'],
    [{ hook_event_name: 'AfterTool', tool_name: 'read_file' }, 'activity'],
    [{ hook_event_name: 'Notification', notification_type: 'ToolPermission' }, 'asking'],
    [{ hook_event_name: 'AfterAgent' }, 'turn-ended'],
  ])('%j is %s', (raw, signal) => {
    expect(normalize('gemini', raw)?.signal).toBe(signal)
  })

  it('takes only a permission notification as a wait, and names the file a tool reads', () => {
    expect(
      normalize('gemini', { hook_event_name: 'Notification', notification_type: 'Other' }),
    ).toBe(undefined)
    expect(
      normalize('gemini', {
        hook_event_name: 'BeforeTool',
        cwd: '/w',
        tool_name: 'read_file',
        tool_input: { absolute_path: '/w/src/a.ts' },
      })?.summary,
    ).toBe('read_file src/a.ts')
  })
})

describe('GitHub Copilot CLI', () => {
  const hook = (event: string, rest: Record<string, unknown> = {}) =>
    normalize('copilot', { [COPILOT_EVENT_FIELD]: event, cwd: '/w', ...rest })

  it.each([
    ['userPromptSubmitted', 'prompt'],
    ['preToolUse', 'activity'],
    ['postToolUseFailure', 'activity'],
    ['permissionRequest', 'asking'],
    ['agentStop', 'turn-ended'],
  ])('%s is %s', (event, signal) => {
    expect(hook(event)?.signal).toBe(signal)
  })

  it('fails only on an error Copilot cannot get over', () => {
    expect(hook('errorOccurred', { recoverable: true })).toBeUndefined()
    expect(hook('errorOccurred', { recoverable: false })?.signal).toBe('failed')
  })

  it('reads the tool arguments as JSON text', () => {
    expect(
      hook('preToolUse', { toolName: 'bash', toolArgs: '{"command":"ls -la"}' })?.summary,
    ).toBe('bash: ls -la')
  })

  it('needs the event that the hook command names', () => {
    expect(() => normalize('copilot', { cwd: '/w' })).toThrow()
  })
})

describe('Cursor', () => {
  it.each([
    [{ hook_event_name: 'beforeSubmitPrompt' }, 'prompt'],
    [{ hook_event_name: 'beforeShellExecution', command: 'ls' }, 'activity'],
    [{ hook_event_name: 'afterFileEdit', file_path: '/w/a.ts' }, 'activity'],
    [{ hook_event_name: 'stop', status: 'completed' }, 'turn-ended'],
    [{ hook_event_name: 'stop', status: 'aborted' }, 'turn-ended'],
    [{ hook_event_name: 'stop', status: 'error' }, 'failed'],
  ])('%j is %s', (raw, signal) => {
    expect(normalize('cursor', raw)?.signal).toBe(signal)
  })

  it('finds the folder in workspace_roots when there is no cwd', () => {
    const hook = normalize('cursor', {
      hook_event_name: 'beforeShellExecution',
      workspace_roots: ['/w'],
      conversation_id: 'c1',
      command: 'pnpm test',
    })
    expect(hook).toMatchObject({ cwd: '/w', summary: 'Shell: pnpm test' })
    expect(hook?.details).toMatchObject({ sessionId: 'c1' })
  })
})
