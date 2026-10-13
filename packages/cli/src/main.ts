#!/usr/bin/env node
import * as nodeModule from 'node:module'

// Caches the compiled code on disk, so the next start is faster. Older runtimes lack it.
if (typeof nodeModule.enableCompileCache === 'function') {
  nodeModule.enableCompileCache()
}

// Reads all of stdin, or nothing when the command is run by hand in a terminal.
async function readStdin(): Promise<string> {
  if (process.stdin.isTTY) {
    return ''
  }
  const chunks: Buffer[] = []
  for await (const chunk of process.stdin) {
    chunks.push(Buffer.from(chunk))
  }
  return Buffer.concat(chunks).toString('utf8')
}

const stdout = (text: string) => process.stdout.write(text)
const stderr = (text: string) => process.stderr.write(text)

// `natamux hook` runs on every tool call of an agent, so it loads only forward-hook.js, not the
// command parser and what it imports (zod).
if (process.argv[2] === 'hook') {
  const { runHookCommand } = await import('./forward-hook.js')
  process.exitCode = await runHookCommand(process.argv.slice(3), {
    env: process.env,
    stdout,
    stderr,
    readStdin,
    fetch,
  })
} else {
  const { runCli } = await import('./run.js')
  process.exitCode = await runCli(process.argv.slice(2), {
    env: process.env,
    io: { stdout, stderr },
    readStdin,
  })
}
