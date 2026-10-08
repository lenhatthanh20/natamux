import { describe, expect, it } from 'vitest'
import { portRange } from './ports.js'

describe('portRange', () => {
  it.each([
    ['T-1', 20_010, 20_019],
    ['T-0042', 20_420, 20_429],
    ['T-1001', 20_010, 20_019],
    ['T-0', 20_000, 20_009],
  ])('gives %s the ports %i to %i', (taskId, first, last) => {
    expect(portRange(taskId)).toEqual({ first, last })
  })
})
