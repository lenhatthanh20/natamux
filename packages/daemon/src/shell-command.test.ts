import { execFileSync } from 'node:child_process'
import { existsSync, mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { describe, expect, it } from 'vitest'
import { shellCommand } from './shell-command.js'

const only =
  (...files: string[]) =>
  (file: string) =>
    files.includes(file)

describe('shellCommand', () => {
  it.each(['darwin', 'linux'] as const)('starts a login shell on %s', (platform) => {
    expect(shellCommand({ SHELL: '/bin/zsh' }, platform, only('/bin/zsh'))).toEqual({
      file: '/bin/zsh',
      args: ['-l'],
    })
  })

  it.each(['/bin/bash', '/usr/local/bin/fish', '/Users/me/My Tools/zsh'])(
    'passes %s as one file, with the login flag',
    (shell) => {
      expect(shellCommand({ SHELL: shell }, 'darwin', only(shell))).toEqual({
        file: shell,
        args: ['-l'],
      })
    },
  )

  it.each(['', '   '])('falls back to bash when $SHELL is %j', (shell) => {
    expect(shellCommand({ SHELL: shell }, 'linux', only('/bin/bash'))).toEqual({
      file: '/bin/bash',
      args: ['-l'],
    })
  })

  it('falls back to bash, then sh, when $SHELL is not there', () => {
    expect(shellCommand({}, 'darwin', only('/bin/bash'))).toEqual({
      file: '/bin/bash',
      args: ['-l'],
    })
    expect(shellCommand({ SHELL: '/gone/zsh' }, 'linux', only('/bin/bash'))).toEqual({
      file: '/bin/bash',
      args: ['-l'],
    })
    expect(shellCommand({ SHELL: '/gone/zsh' }, 'linux', only())).toEqual({
      file: '/bin/sh',
      args: ['-l'],
    })
  })

  it('starts PowerShell with no arguments on Windows', () => {
    expect(shellCommand({ SHELL: '/bin/zsh' }, 'win32', only('/bin/zsh'))).toEqual({
      file: 'powershell.exe',
      args: [],
    })
  })

  // The fallback shells must accept the flag. A fresh HOME keeps the CI machine's dotfiles out.
  for (const file of ['/bin/sh', '/bin/bash'].filter((path) => existsSync(path))) {
    it(`${file} accepts the login flag`, () => {
      const home = mkdtempSync(join(tmpdir(), 'natamux-shell-'))
      try {
        const out = execFileSync(file, ['-l', '-c', 'echo ok'], {
          encoding: 'utf8',
          env: { HOME: home, PATH: '/usr/bin:/bin' },
        })
        expect(out.trim()).toBe('ok')
      } finally {
        rmSync(home, { recursive: true, force: true })
      }
    })
  }
})
