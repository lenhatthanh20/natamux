import { readFileSync } from 'node:fs'
import {
  TASK_ID_HEADER as CORE_TASK_ID_HEADER,
  TERMINAL_HEADER as CORE_TERMINAL_HEADER,
} from '@natamux/core'
import { describe, expect, it } from 'vitest'
import type { FetchFunction } from './client.js'
import { forwardHook, runHookCommand, TASK_ID_HEADER, TERMINAL_HEADER } from './forward-hook.js'
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
      env: options.env ?? { NATAMUX_TOKEN: 'secret' },
      readStdin: async () => options.stdin ?? HOOK_JSON,
      fetch: fetchFunction,
      debug: (text) => debug.push(text),
    })
  return { calls, debug, run }
}

describe('forwardHook', () => {
  it('does nothing without NATAMUX_TOKEN, because the session is not one of Natamux', async () => {
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

  it('names the task when NATAMUX_TASK_ID is set, and sends no header when it is not', async () => {
    const named = setup({ env: { NATAMUX_TOKEN: 'secret', NATAMUX_TASK_ID: 'T-0007' } })
    await named.run()
    expect(named.calls[0]?.headers.get('x-natamux-task')).toBe('T-0007')

    const unnamed = setup()
    await unnamed.run()
    expect(unnamed.calls[0]?.headers.has('x-natamux-task')).toBe(false)
  })

  it('uses NATAMUX_URL and drops a trailing slash', async () => {
    const { calls, run } = setup({
      env: { NATAMUX_TOKEN: 'secret', NATAMUX_URL: 'http://example.test:9000/' },
    })

    await run()

    expect(calls[0]?.url).toBe('http://example.test:9000/hooks/claude-code')
  })

  it('sends nothing when stdin is empty', async () => {
    const { calls, run } = setup({ stdin: '  \n' })

    await run()

    expect(calls).toEqual([])
  })

  it('sends stdin that is not JSON as it is, and leaves the verdict to the daemon', async () => {
    const { calls, run } = setup({ stdin: '{not json' })

    await run()

    expect(calls[0]?.body).toBe('{not json')
  })

  it('sends a very large stdin whole', async () => {
    const big = `{"output":"${'x'.repeat(5 * 1024 * 1024)}"}`
    const { calls, run } = setup({ stdin: big })

    await run()

    expect(calls[0]?.body).toBe(big)
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

  it('says what went wrong when NATAMUX_DEBUG is set', async () => {
    const answers = setup({
      env: { NATAMUX_TOKEN: 'secret', NATAMUX_DEBUG: '1' },
      respond: () => new Response('no', { status: 500 }),
    })
    await answers.run()
    expect(answers.debug.join('')).toContain('the daemon answered 500')

    const unreachable = setup({
      env: { NATAMUX_TOKEN: 'secret', NATAMUX_DEBUG: '1' },
      respond: () => {
        throw new TypeError('fetch failed')
      },
    })
    await unreachable.run()
    expect(unreachable.debug.join('')).toContain('fetch failed')
  })
})

describe('natamux hook', () => {
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
    const result = await runHook(['hook', 'claude-code'], { NATAMUX_TOKEN: 'secret' })

    expect(result.code).toBe(EXIT_OK)
    expect(result.stdout).toBe('')
    expect(result.stderr).toBe('')
    expect(result.calls).toEqual(['http://127.0.0.1:4317/hooks/claude-code'])
  })

  it('names the event of a Copilot hook in the address', async () => {
    const result = await runHook(['hook', 'copilot', 'agentStop'], { NATAMUX_TOKEN: 'secret' })

    expect(result.calls).toEqual(['http://127.0.0.1:4317/hooks/copilot?event=agentStop'])
  })

  it('answers {} for the agents that read JSON, even outside a Natamux session', async () => {
    for (const agent of ['cursor', 'gemini']) {
      expect((await runHook(['hook', agent], {})).stdout).toBe('{}\n')
    }
    expect((await runHook(['hook', 'codex'], {})).stdout).toBe('')
  })

  it('exits 0 and prints nothing when the daemon is down or blocks the hook', async () => {
    for (const respond of [
      () => {
        throw new TypeError('fetch failed')
      },
      () => new Response('blocked', { status: 403 }),
    ]) {
      let stdout = ''
      let stderr = ''
      const code = await runHookCommand(['claude-code'], {
        env: { NATAMUX_TOKEN: 'secret' },
        stdout: (text) => {
          stdout += text
        },
        stderr: (text) => {
          stderr += text
        },
        readStdin: async () => HOOK_JSON,
        fetch: async () => respond(),
      })

      expect(code).toBe(EXIT_OK)
      expect(stdout).toBe('')
      expect(stderr).toBe('')
    }
  })

  it('exits 0 and prints nothing outside a Natamux session', async () => {
    const result = await runHook(['hook', 'claude-code'], {})

    expect(result.code).toBe(EXIT_OK)
    expect(result.stdout).toBe('')
    expect(result.calls).toEqual([])
  })

  it('exits 0 even for a wrong command, so the agent is not blocked', async () => {
    const result = await runHook(['hook'], { NATAMUX_TOKEN: 'secret' })

    expect(result.code).toBe(EXIT_OK)
    expect(result.stdout).toBe('')
    expect(result.stderr).toContain('agent name is required')
    expect(result.calls).toEqual([])
  })

  it.each([[['hook', 'Claude Code']], [['hook', 'a/b']], [['hook', '1x']]])(
    'exits 0 and names the problem for the agent name in %j',
    async (argv) => {
      const result = await runHook(argv, { NATAMUX_TOKEN: 'secret' })

      expect(result.code).toBe(EXIT_OK)
      expect(result.stderr).toContain('agent name is required')
      expect(result.calls).toEqual([])
    },
  )

  it('takes the event of a Copilot hook, and nothing more', async () => {
    const env = { NATAMUX_TOKEN: 'secret' }
    for (const argv of [
      ['hook', 'copilot', 'agentStop', 'extra'],
      ['hook', 'copilot', 'a/b'],
    ]) {
      const result = await runHook(argv, env)
      expect(result.code).toBe(EXIT_OK)
      expect(result.stderr).toContain('Unexpected argument')
      expect(result.calls).toEqual([])
    }
  })

  it('names only the extra arguments, as the full parser did', async () => {
    const result = await runHook(['hook', 'copilot', 'agentStop', 'a', 'b'], {})

    expect(result.stderr).toBe('Unexpected argument: a b\nRun "natamux help" for usage.\n')
  })
})

describe('the fast hook path', () => {
  const dir = new URL('.', import.meta.url)

  // Value imports and re-exports of a source file, which is what Node loads at start-up. It also
  // matches an import that biome breaks over several lines.
  const VALUE_IMPORT = /^(?:import|export)\s(?!type\s)[^']*?\sfrom\s'([^']+)'/gm

  function valueImports(file: string): string[] {
    const source = readFileSync(new URL(file, dir), 'utf8')
    return [...source.matchAll(VALUE_IMPORT)].map((match) => match[1] ?? '')
  }

  it('finds the imports of a file', () => {
    expect(valueImports('./forward-hook.ts')).toContain('./default-url.js')
    expect(valueImports('./forward-hook.test.ts')).toContain('@natamux/core')
  })

  it('keeps the header names of @natamux/core', () => {
    expect(TASK_ID_HEADER).toBe(CORE_TASK_ID_HEADER)
    expect(TERMINAL_HEADER).toBe(CORE_TERMINAL_HEADER)
  })

  it('loads no package and no command parser, only files that import none', () => {
    const seen = new Set<string>()
    const queue = ['./forward-hook.ts']
    for (const file of queue) {
      if (seen.has(file)) {
        continue
      }
      seen.add(file)
      for (const target of valueImports(file)) {
        expect(target.startsWith('./') || target.startsWith('node:')).toBe(true)
        if (target.startsWith('./')) {
          queue.push(target.replace(/\.js$/, '.ts'))
        }
      }
    }
    expect(seen).not.toContain('./command.ts')
    expect(seen).not.toContain('./run.ts')
  })

  it('has main.ts load run.js only on demand', () => {
    expect(valueImports('./main.ts')).not.toContain('./run.js')
  })

  it('has main.ts send only `hook` to forward-hook.js, and every other command to run.js', () => {
    const source = readFileSync(new URL('./main.ts', dir), 'utf8')
    expect(source).toContain("process.argv[2] === 'hook'")
    expect(source).toMatch(/await import\('\.\/forward-hook\.js'\)/)
    expect(source).toMatch(/else \{\s*const \{ runCli \} = await import\('\.\/run\.js'\)/)
  })

  it('has main.ts turn on the compile cache first, only where the runtime has it', () => {
    const source = readFileSync(new URL('./main.ts', dir), 'utf8')
    const guard = source.indexOf("typeof nodeModule.enableCompileCache === 'function'")
    expect(guard).toBeGreaterThan(-1)
    expect(source.indexOf('nodeModule.enableCompileCache()')).toBeGreaterThan(guard)
    expect(source.indexOf('nodeModule.enableCompileCache()')).toBeLessThan(
      source.indexOf('await import('),
    )
  })
})
