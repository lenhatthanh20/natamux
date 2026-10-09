import { describe, expect, it } from 'vitest'
import type { FetchFunction } from './client.js'
import { forwardHook } from './forward-hook.js'
import { EXIT_OK, runCli } from './run.js'

type Call = { url: string; method: string; headers: Headers; body: string }

const HOOK_JSON = '{"hook_event_name":"Stop","cwd":"/work/T-0001"}'

function setup(
  options: {
    env?: Record<string, string | undefined>
    stdin?: string
    respond?: () => Response
  } = {},
) {
  const calls: Call[] = []
  const debug: string[] = []
  const fetchFunction: FetchFunction = async (url, init) => {
    calls.push({
      url,
      method: init?.method ?? 'GET',
      headers: new Headers(init?.headers),
      body: String(init?.body),
    })
    return (options.respond ?? (() => new Response('{}')))()
  }
  const run = () =>
    forwardHook({
      agent: 'claude-code',
      env: options.env ?? { HELMSMAN_TOKEN: 'secret' },
      readStdin: async () => options.stdin ?? HOOK_JSON,
      fetch: fetchFunction,
      debug: (text) => debug.push(text),
    })
  return { calls, debug, run }
}

describe('forwardHook', () => {
  it('does nothing without HELMSMAN_TOKEN, because the session is not one of Helmsman', async () => {
    const { calls, debug, run } = setup({ env: {} })

    await run()

    expect(calls).toEqual([])
    expect(debug).toEqual([])
  })

  it('drops the byte order mark that Cursor puts first on Windows', async () => {
    const { calls, run } = setup({ stdin: `\uFEFF${HOOK_JSON}` })

    await run()

    expect(calls[0]?.body).toBe(HOOK_JSON)
  })

  it('posts the JSON of the hook as it is, with the token', async () => {
    const { calls, run } = setup()

    await run()

    expect(calls).toHaveLength(1)
    expect(calls[0]?.url).toBe('http://127.0.0.1:4317/hooks/claude-code')
    expect(calls[0]?.method).toBe('POST')
    expect(calls[0]?.body).toBe(HOOK_JSON)
    expect(calls[0]?.headers.get('authorization')).toBe('Bearer secret')
    expect(calls[0]?.headers.get('content-type')).toBe('application/json')
  })

  it('names the task when HELMSMAN_TASK_ID is set, and sends no header when it is not', async () => {
    const named = setup({ env: { HELMSMAN_TOKEN: 'secret', HELMSMAN_TASK_ID: 'T-0007' } })
    await named.run()
    expect(named.calls[0]?.headers.get('x-helmsman-task')).toBe('T-0007')

    const unnamed = setup()
    await unnamed.run()
    expect(unnamed.calls[0]?.headers.has('x-helmsman-task')).toBe(false)
  })

  it('uses HELMSMAN_URL and drops a trailing slash', async () => {
    const { calls, run } = setup({
      env: { HELMSMAN_TOKEN: 'secret', HELMSMAN_URL: 'http://example.test:9000/' },
    })

    await run()

    expect(calls[0]?.url).toBe('http://example.test:9000/hooks/claude-code')
  })

  it('sends nothing when stdin is empty', async () => {
    const { calls, run } = setup({ stdin: '  \n' })

    await run()

    expect(calls).toEqual([])
  })

  it('stays silent when the daemon cannot be reached or answers with an error', async () => {
    const answers = setup({ respond: () => new Response('no', { status: 500 }) })
    await expect(answers.run()).resolves.toBeUndefined()
    expect(answers.debug).toEqual([])

    const unreachable = setup({
      respond: () => {
        throw new TypeError('fetch failed')
      },
    })
    await expect(unreachable.run()).resolves.toBeUndefined()
    expect(unreachable.debug).toEqual([])
  })

  it('says what went wrong when HELMSMAN_DEBUG is set', async () => {
    const answers = setup({
      env: { HELMSMAN_TOKEN: 'secret', HELMSMAN_DEBUG: '1' },
      respond: () => new Response('no', { status: 500 }),
    })
    await answers.run()
    expect(answers.debug.join('')).toContain('the daemon answered 500')

    const unreachable = setup({
      env: { HELMSMAN_TOKEN: 'secret', HELMSMAN_DEBUG: '1' },
      respond: () => {
        throw new TypeError('fetch failed')
      },
    })
    await unreachable.run()
    expect(unreachable.debug.join('')).toContain('fetch failed')
  })
})

describe('helmsman hook', () => {
  async function runHook(argv: string[], env: Record<string, string | undefined>) {
    const calls: string[] = []
    let stdout = ''
    let stderr = ''
    const code = await runCli(argv, {
      env,
      io: {
        stdout: (text) => {
          stdout += text
        },
        stderr: (text) => {
          stderr += text
        },
      },
      readStdin: async () => HOOK_JSON,
      fetch: async (url) => {
        calls.push(url)
        return new Response('{}')
      },
    })
    return { code, stdout, stderr, calls }
  }

  it('forwards the hook, exits 0 and prints nothing', async () => {
    const result = await runHook(['hook', 'claude-code'], { HELMSMAN_TOKEN: 'secret' })

    expect(result.code).toBe(EXIT_OK)
    expect(result.stdout).toBe('')
    expect(result.stderr).toBe('')
    expect(result.calls).toEqual(['http://127.0.0.1:4317/hooks/claude-code'])
  })

  it('names the event of a Copilot hook in the address', async () => {
    const result = await runHook(['hook', 'copilot', 'agentStop'], { HELMSMAN_TOKEN: 'secret' })

    expect(result.calls).toEqual(['http://127.0.0.1:4317/hooks/copilot?event=agentStop'])
  })

  it('answers {} for the agents that read JSON, even outside a Helmsman session', async () => {
    for (const agent of ['cursor', 'gemini']) {
      expect((await runHook(['hook', agent], {})).stdout).toBe('{}\n')
    }
    expect((await runHook(['hook', 'codex'], {})).stdout).toBe('')
  })

  it('exits 0 and prints nothing outside a Helmsman session', async () => {
    const result = await runHook(['hook', 'claude-code'], {})

    expect(result.code).toBe(EXIT_OK)
    expect(result.stdout).toBe('')
    expect(result.calls).toEqual([])
  })

  it('exits 0 even for a wrong command, so the agent is not blocked', async () => {
    const result = await runHook(['hook'], { HELMSMAN_TOKEN: 'secret' })

    expect(result.code).toBe(EXIT_OK)
    expect(result.stdout).toBe('')
    expect(result.stderr).toContain('agent name is required')
    expect(result.calls).toEqual([])
  })
})
