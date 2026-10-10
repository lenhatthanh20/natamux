import { mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { SETUP_SCRIPT, SETUP_SCRIPT_WINDOWS, setupCommand } from './setup-script.js'

let folder: string

beforeEach(() => {
  folder = mkdtempSync(join(tmpdir(), 'natamux-setup-'))
})

afterEach(() => {
  rmSync(folder, { recursive: true, force: true })
})

describe('setupCommand', () => {
  it('runs the script with sh when the worktree has one', () => {
    expect(setupCommand(folder, 'linux')).toBeNull()

    writeFileSync(join(folder, SETUP_SCRIPT), 'pnpm install\n')

    expect(setupCommand(folder, 'linux')).toBe('sh ./.natamux-setup')
    expect(setupCommand(folder, 'darwin')).toBe('sh ./.natamux-setup')
  })

  it('runs the PowerShell script on Windows', () => {
    writeFileSync(join(folder, SETUP_SCRIPT), 'pnpm install\n')
    expect(setupCommand(folder, 'win32')).toBeNull()

    writeFileSync(join(folder, SETUP_SCRIPT_WINDOWS), 'pnpm install\n')
    expect(setupCommand(folder, 'win32')).toBe('& ./.natamux-setup.ps1')
  })
})
