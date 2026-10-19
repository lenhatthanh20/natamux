import { join } from 'node:path'
import { describe, expect, it } from 'vitest'
import { ghosttyConfigPaths, parseGhosttyFont, readGhosttyFont } from '../ghostty-config.js'

describe('Ghostty config', () => {
  it('is read from the XDG folder, and on macOS from Application Support after it', () => {
    expect(ghosttyConfigPaths({}, '/home/me', 'linux')).toEqual([
      join('/home/me', '.config', 'ghostty', 'config'),
      join('/home/me', '.config', 'ghostty', 'config.ghostty'),
    ])
    const support = join('/Users/me', 'Library', 'Application Support', 'com.mitchellh.ghostty')
    expect(ghosttyConfigPaths({ XDG_CONFIG_HOME: '/xdg' }, '/Users/me', 'darwin')).toEqual([
      join('/xdg', 'ghostty', 'config'),
      join('/xdg', 'ghostty', 'config.ghostty'),
      join(support, 'config'),
      join(support, 'config.ghostty'),
    ])
  })

  it('gives the first font family, without quotes, and the last size', () => {
    const text = [
      '# My terminal',
      'theme = Catppuccin Mocha',
      'font-family = "JetBrains Mono"',
      'font-family = Symbols Nerd Font',
      'font-size = 13',
      'font-size = 14.5',
      '# font-family = Comic Mono',
    ].join('\n')
    expect(parseGhosttyFont(text)).toEqual({ family: 'JetBrains Mono', size: 14.5 })
  })

  it('starts the family list again after an empty value', () => {
    const text = 'font-family = Menlo\nfont-family = ""\nfont-family = Iosevka'
    expect(parseGhosttyFont(text)).toEqual({ family: 'Iosevka' })
  })

  it('ignores a size that is not a number', () => {
    expect(parseGhosttyFont('font-size = big')).toEqual({})
  })

  it('lets a later file set the font again, and keeps what it does not set', () => {
    const files: Record<string, string> = {
      [join('/home/me', '.config', 'ghostty', 'config')]: 'font-family = Menlo\nfont-size = 12\n',
      [join('/home/me', '.config', 'ghostty', 'config.ghostty')]: 'font-family = Iosevka\n',
    }
    const read = (path: string) => {
      const text = files[path]
      if (text === undefined) {
        throw new Error('ENOENT')
      }
      return text
    }
    expect(readGhosttyFont({}, '/home/me', 'linux', read)).toEqual({ family: 'Iosevka', size: 12 })
  })

  it('gives nothing when Ghostty has no config or sets no font', () => {
    const none = () => {
      throw new Error('ENOENT')
    }
    expect(readGhosttyFont({}, '/home/me', 'linux', none)).toBeUndefined()
    expect(readGhosttyFont({}, '/home/me', 'linux', () => 'theme = dark\n')).toBeUndefined()
  })
})
