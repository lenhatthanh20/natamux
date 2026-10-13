import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'

describe('entry.ts', () => {
  it('turns on the compile cache, only where the runtime has it, before it loads main.js', () => {
    const source = readFileSync(new URL('./entry.ts', import.meta.url), 'utf8')
    const guard = source.indexOf("typeof nodeModule.enableCompileCache === 'function'")
    const call = source.indexOf('nodeModule.enableCompileCache()')
    expect(guard).toBeGreaterThan(-1)
    expect(call).toBeGreaterThan(guard)
    expect(call).toBeLessThan(source.indexOf("await import('./main.js')"))
  })
})

describe('the launchers of the daemon', () => {
  it('start dist/entry.js', () => {
    const pkg = JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8'))
    expect(pkg.scripts.start).toBe('node dist/entry.js')
    const file = new URL('../../../docker/prod.Dockerfile', import.meta.url)
    const docker = readFileSync(file, 'utf8')
    expect(docker).toContain('packages/daemon/dist/entry.js')
  })
})
