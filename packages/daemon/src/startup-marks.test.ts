import { describe, expect, it } from 'vitest'
import { startupLine } from './startup-marks.js'

describe('startupLine', () => {
  it('puts every mark with its time on one line', () => {
    const line = startupLine([
      ['module-start', 120],
      ['init', 200],
      ['build-server', 260],
      ['listen', 275],
    ])
    expect(line).toBe(
      'Startup, ms since the process began: module-start 120, init 200, build-server 260, listen 275',
    )
    expect(line).not.toContain('\n')
  })
})
