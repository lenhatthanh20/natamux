import { CLAUDE_CODE_HOOK_EVENTS } from '@natamux/core'
import { describe, expect, it } from 'vitest'
import { claudeCodeHookSettings } from './hooks.js'
import { EXIT_OK, runCli } from './run.js'

describe('claudeCodeHookSettings', () => {
  const settings = claudeCodeHookSettings('http://127.0.0.1:4317/')

  it('has a hook for every event Natamux listens to', () => {
    expect(Object.keys(settings.hooks)).toEqual([...CLAUDE_CODE_HOOK_EVENTS])
  })

  it('posts to the daemon and drops a trailing slash of its address', () => {
    for (const entries of Object.values(settings.hooks)) {
      expect(entries[0]?.hooks[0]).toMatchObject({
        type: 'http',
        url: 'http://127.0.0.1:4317/hooks/claude-code',
      })
    }
  })

  it('takes the token, the task id and the terminal from the environment, never writing them in', () => {
    const hook = settings.hooks.Stop?.[0]?.hooks[0]
    expect(hook?.headers).toEqual({
      Authorization: 'Bearer $NATAMUX_TOKEN',
      'X-Natamux-Task': '$NATAMUX_TASK_ID',
      'X-Natamux-Terminal': '$NATAMUX_TERMINAL',
    })
    expect(hook?.allowedEnvVars).toEqual(['NATAMUX_TOKEN', 'NATAMUX_TASK_ID', 'NATAMUX_TERMINAL'])
  })

  it('stops waiting for the daemon after a few seconds', () => {
    expect(settings.hooks.Stop?.[0]?.hooks[0]?.timeout).toBe(5)
  })
})

describe('hooks claude-code', () => {
  async function print(env: Record<string, string | undefined>) {
    let stdout = ''
    const code = await runCli(['hooks', 'claude-code'], {
      env,
      io: {
        stdout: (text) => {
          stdout += text
        },
        stderr: () => undefined,
      },
      fetch: async () => {
        throw new Error('The daemon must not be called')
      },
    })
    return { code, settings: JSON.parse(stdout) }
  }

  it('prints JSON without a token and without calling the daemon', async () => {
    const { code, settings } = await print({})
    expect(code).toBe(EXIT_OK)
    expect(settings.hooks.Stop[0].hooks[0].url).toBe('http://127.0.0.1:4317/hooks/claude-code')
  })

  it('uses NATAMUX_URL for the address', async () => {
    const { settings } = await print({ NATAMUX_URL: 'http://example.test:9000' })
    expect(settings.hooks.Notification[0].hooks[0].url).toBe(
      'http://example.test:9000/hooks/claude-code',
    )
  })

  it('does not print the token of the environment', async () => {
    const { settings } = await print({ NATAMUX_TOKEN: 'super-secret-value' })
    expect(JSON.stringify(settings)).not.toContain('super-secret-value')
  })
})
