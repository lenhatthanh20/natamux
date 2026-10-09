import { chmodSync, mkdirSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'

// Writes the `helmsman` command that the agents run (`helmsman done`, `helmsman ask`). It starts
// the CLI with the app's own runtime, so no separate Node.js is needed.
export function writeCliShim(
  binDir: string,
  runtime: string,
  cliMain: string,
  platform: NodeJS.Platform,
): string {
  mkdirSync(binDir, { recursive: true })
  if (platform === 'win32') {
    const file = join(binDir, 'helmsman.cmd')
    writeFileSync(
      file,
      `@echo off\r\nset ELECTRON_RUN_AS_NODE=1\r\n"${runtime}" "${cliMain}" %*\r\n`,
    )
    return file
  }
  const file = join(binDir, 'helmsman')
  const quote = (value: string) => `'${value.replaceAll("'", `'\\''`)}'`
  writeFileSync(
    file,
    `#!/bin/sh\nELECTRON_RUN_AS_NODE=1 exec ${quote(runtime)} ${quote(cliMain)} "$@"\n`,
  )
  chmodSync(file, 0o755)
  return file
}
