import { describe, expect, it } from 'vitest'
import { shellCommand } from './shell-command.js'

const only = (...files: string[]) => (file: string) => files.includes(file)

describe('shellCommand', () => {
  it.each(['darwin', 'linux'] as const)('starts a login shell on %s', (platform) => {
    expect(shellCommand({ SHELL: '/bin/zsh' }, platform, only('/bin/zsh'))).toEqual({
      file: '/bin/zsh',
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
})
