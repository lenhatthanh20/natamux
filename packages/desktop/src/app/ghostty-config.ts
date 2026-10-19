import { readFileSync } from 'node:fs'
import { join } from 'node:path'

export type GhosttyFont = { family?: string; size?: number }

// Where Ghostty reads its config, in its own order: a later file wins.
// See https://ghostty.org/docs/config#macos-specific-path-(macos-only).
export function ghosttyConfigPaths(
  env: NodeJS.ProcessEnv,
  home: string,
  platform: NodeJS.Platform,
): string[] {
  const xdg = env.XDG_CONFIG_HOME?.trim() || join(home, '.config')
  const paths = [join(xdg, 'ghostty', 'config'), join(xdg, 'ghostty', 'config.ghostty')]
  if (platform === 'darwin') {
    const support = join(home, 'Library', 'Application Support', 'com.mitchellh.ghostty')
    paths.push(join(support, 'config'), join(support, 'config.ghostty'))
  }
  return paths
}

function unquote(value: string): string {
  const trimmed = value.trim()
  return trimmed.length >= 2 && trimmed.startsWith('"') && trimmed.endsWith('"')
    ? trimmed.slice(1, -1)
    : trimmed
}

// The font of a Ghostty config: the first `font-family` (an empty one clears the list, as in
// Ghostty) and the last `font-size`.
export function parseGhosttyFont(text: string, font: GhosttyFont = {}): GhosttyFont {
  const result = { ...font }
  let familyInThisFile = false
  for (const line of text.split(/\r?\n/)) {
    const match = /^\s*([a-z-]+)\s*=(.*)$/.exec(line)
    if (!match || line.trimStart().startsWith('#')) {
      continue
    }
    const [, key, raw = ''] = match
    const value = unquote(raw)
    if (key === 'font-family') {
      if (value === '') {
        delete result.family
        familyInThisFile = false
      } else if (!familyInThisFile) {
        // A later file sets the list again; later lines in one file only add fallbacks.
        result.family = value
        familyInThisFile = true
      }
    } else if (key === 'font-size') {
      const size = Number(value)
      if (Number.isFinite(size) && size > 0) {
        result.size = size
      }
    }
  }
  return result
}

// Ghostty's font on this machine, or undefined when Ghostty has no config or sets no font.
export function readGhosttyFont(
  env: NodeJS.ProcessEnv,
  home: string,
  platform: NodeJS.Platform,
  read: (path: string) => string = (path) => readFileSync(path, 'utf8'),
): GhosttyFont | undefined {
  let font: GhosttyFont = {}
  for (const path of ghosttyConfigPaths(env, home, platform)) {
    try {
      font = parseGhosttyFont(read(path), font)
    } catch {
      // No such file: Ghostty skips it too.
    }
  }
  return font.family === undefined && font.size === undefined ? undefined : font
}
