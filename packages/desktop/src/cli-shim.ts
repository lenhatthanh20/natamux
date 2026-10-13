import { chmodSync, mkdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

// `nmx` is the short name people type; agents are told to run `natamux`.
const CLI_NAMES = ['natamux', 'nmx'] as const

// Writes the `natamux` and `nmx` commands that the agents run (`natamux done`, `natamux ask`).
// They start the CLI with the app's own runtime, so no separate Node.js is needed. Returns the
// path of `natamux`.
export function writeCliShim(
  binDir: string,
  runtime: string,
  cliMain: string,
  platform: NodeJS.Platform,
): string {
  mkdirSync(binDir, { recursive: true })
  const [main, ...short] = CLI_NAMES
  for (const name of short) {
    writeOne(binDir, name, runtime, cliMain, platform)
  }
  return writeOne(binDir, main, runtime, cliMain, platform)
}

function writeOne(
  binDir: string,
  name: string,
  runtime: string,
  cliMain: string,
  platform: NodeJS.Platform,
): string {
  if (platform === 'win32') {
    // setlocal: run from cmd.exe, the variable would stay set there and turn the next Electron app
    // started in it into plain Node.js.
    const file = join(binDir, `${name}.cmd`)
    writeFileSync(
      file,
      `@echo off\r\nsetlocal\r\nset ELECTRON_RUN_AS_NODE=1\r\n"${runtime}" "${cliMain}" %*\r\n`,
    )
    return file
  }
  const file = join(binDir, name)
  const quote = (value: string) => `'${value.replaceAll("'", `'\\''`)}'`
  writeFileSync(
    file,
    `#!/bin/sh\nELECTRON_RUN_AS_NODE=1 exec ${quote(runtime)} ${quote(cliMain)} "$@"\n`,
  )
  chmodSync(file, 0o755)
  return file
}
