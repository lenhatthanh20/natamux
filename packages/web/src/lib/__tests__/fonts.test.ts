import { describe, expect, it } from 'vitest'
import diff from '../../components/MonacoDiff.tsx?raw'
import file from '../../components/MonacoFile.tsx?raw'
import setup from '../monaco-setup.ts?raw'

describe('the code views', () => {
  it('use the font of the terminal, so code looks the same in a diff and in a shell', () => {
    for (const source of [diff, file]) {
      expect(source).toContain('fontFamily: TERMINAL_FONT')
      expect(source).not.toContain('MONO_FONT')
    }
  })

  it('measure the font again once JetBrains Mono has loaded', () => {
    expect(setup).toContain('monaco.editor.remeasureFonts()')
  })
})
