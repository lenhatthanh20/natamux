import { readFileSync } from 'node:fs'
import { describe, expect, it } from 'vitest'

describe('entry.ts', () => {
  it('turns on the compile cache, only where the runtime has it, before it loads main.js', () => {
    const source = readFileSync(new URL('../entry.ts', import.meta.url), 'utf8')
    const guard = source.indexOf("typeof nodeModule.enableCompileCache === 'function'")
    const call = source.indexOf('nodeModule.enableCompileCache()')
    expect(guard).toBeGreaterThan(-1)
    expect(call).toBeGreaterThan(guard)
    expect(call).toBeLessThan(source.indexOf("await import('./main.js')"))
  })
})

describe('paths.ts', () => {
  it('starts the daemon from its entry, not from main.js', () => {
    const source = readFileSync(new URL('../daemon/paths.ts', import.meta.url), 'utf8')
    expect(source).toContain("'entry.js'")
    expect(source).not.toMatch(/daemonMain:.*'main\.js'/)
  })

  it('has package.json start the app from its entry', () => {
    const pkg = JSON.parse(readFileSync(new URL('../../package.json', import.meta.url), 'utf8'))
    expect(pkg.main).toBe('./dist/entry.js')
  })
})
