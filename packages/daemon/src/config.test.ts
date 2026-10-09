import { join } from 'node:path'
import { describe, expect, it } from 'vitest'
import { DEFAULT_PORT, LOOPBACK_HOST, loadConfig } from './config.js'

describe('loadConfig', () => {
  it('uses loopback, the default port and a data directory under the working directory', () => {
    const config = loadConfig({}, '/work/repo')
    expect(config.host).toBe(LOOPBACK_HOST)
    expect(config.port).toBe(DEFAULT_PORT)
    expect(config.dataDir).toBe(join('/work/repo', '.helmsman'))
  })

  it('listens on all interfaces only when HELMSMAN_HOST says so', () => {
    expect(loadConfig({ HELMSMAN_HOST: '0.0.0.0' }, '/work/repo').host).toBe('0.0.0.0')
    expect(loadConfig({ HELMSMAN_HOST: '  ' }, '/work/repo').host).toBe(LOOPBACK_HOST)
  })

  it('generates a token when none is configured', () => {
    const first = loadConfig({}, '/work/repo')
    const second = loadConfig({}, '/work/repo')
    expect(first.tokenWasGenerated).toBe(true)
    expect(first.token.length).toBeGreaterThanOrEqual(32)
    expect(first.token).not.toBe(second.token)
  })

  it('uses the configured token, port and data directory', () => {
    const config = loadConfig(
      { HELMSMAN_TOKEN: ' secret ', HELMSMAN_PORT: '5000', HELMSMAN_DATA_DIR: '/data' },
      '/work/repo',
    )
    expect(config).toMatchObject({
      token: 'secret',
      tokenWasGenerated: false,
      port: 5000,
      dataDir: '/data',
    })
  })

  it('treats empty values as not set', () => {
    const config = loadConfig(
      { HELMSMAN_TOKEN: '  ', HELMSMAN_PORT: '', HELMSMAN_DATA_DIR: '' },
      '/work/repo',
    )
    expect(config.tokenWasGenerated).toBe(true)
    expect(config.port).toBe(DEFAULT_PORT)
    expect(config.dataDir).toBe(join('/work/repo', '.helmsman'))
  })

  it('uses the working directory as the repository and keeps worktrees in the data directory', () => {
    const config = loadConfig({}, '/work/repo')
    expect(config.repoDir).toBe('/work/repo')
    expect(config.worktreesDir).toBe(join('/work/repo', '.helmsman', 'worktrees'))
  })

  it('follows a data directory that is set, and lets the repository and worktrees be set', () => {
    const moved = loadConfig({ HELMSMAN_DATA_DIR: '/data' }, '/work/repo')
    expect(moved.worktreesDir).toBe(join('/data', 'worktrees'))

    const explicit = loadConfig(
      { HELMSMAN_REPO_DIR: '/other/repo', HELMSMAN_WORKTREES_DIR: '/trees' },
      '/work/repo',
    )
    expect(explicit).toMatchObject({ repoDir: '/other/repo', worktreesDir: '/trees' })
  })

  it('takes the dashboard page from HELMSMAN_WEB_INDEX, and serves none without it', () => {
    expect(loadConfig({}, '/work/repo').webIndexPath).toBeUndefined()
    expect(loadConfig({ HELMSMAN_WEB_INDEX: ' ' }, '/work/repo').webIndexPath).toBeUndefined()
    expect(
      loadConfig({ HELMSMAN_WEB_INDEX: '/web/dist/index.html' }, '/work/repo').webIndexPath,
    ).toBe('/web/dist/index.html')
  })

  it('serves the dashboard from HELMSMAN_WEB_DIR, with its index.html for deep links', () => {
    expect(loadConfig({ HELMSMAN_WEB_DIR: '/app/web' }, '/work/repo')).toMatchObject({
      webDir: '/app/web',
      webIndexPath: join('/app/web', 'index.html'),
    })
    expect(loadConfig({}, '/work/repo').webDir).toBeUndefined()
  })

  it.each(['abc', '1.5', '-1', '70000'])('rejects the port %j', (port) => {
    expect(() => loadConfig({ HELMSMAN_PORT: port }, '/work/repo')).toThrow(/HELMSMAN_PORT/)
  })
})
