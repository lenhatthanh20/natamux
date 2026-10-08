#!/usr/bin/env node
import { runCli } from './run.js'

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

process.exitCode = await runCli(process.argv.slice(2), {
  env: process.env,
  io: {
    stdout: (text) => process.stdout.write(text),
    stderr: (text) => process.stderr.write(text),
  },
  readStdin,
})
