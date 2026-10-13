import { describe, expect, it } from 'vitest'
import { portRange } from './ports.js'

describe('portRange', () => {
  it.each([
    ['T-1', 20_010, 20_019],
    ['T-0042', 20_420, 20_429],
    ['T-1001', 20_010, 20_019],
    ['T-0', 20_000, 20_009],
    ['TASK-0005', 20_050, 20_059],
    ['T-0005', 20_050, 20_059],
    ['TASK-1005', 20_050, 20_059],
    ['ABC-0', 20_000, 20_009],
    ['ABC-12', 20_120, 20_129],
    ['XYZ-34', 20_340, 20_349],
  ])('gives %s the ports %i to %i', (taskId, first, last) => {
    expect(portRange(taskId)).toEqual({ first, last })
  })
})
