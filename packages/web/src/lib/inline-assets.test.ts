import { describe, expect, it } from 'vitest'
import { inlineAsset } from './inline-assets'

describe('inlineAsset', () => {
  it('keeps the file icons as files', () => {
    expect(inlineAsset('/repo/node_modules/material-icon-theme/icons/react.svg')).toBe(false)
  })

  it('keeps them as files with Windows paths too', () => {
    expect(inlineAsset('C:\\repo\\node_modules\\material-icon-theme\\icons\\go.svg')).toBe(false)
  })

  it('leaves every other file to the size limit of Vite', () => {
    expect(inlineAsset('/repo/packages/web/src/logo.svg')).toBeUndefined()
    expect(inlineAsset('/repo/node_modules/@fontsource-variable/geist/files/a.woff2')).toBe(
      undefined,
    )
  })
})
