import { describe, expect, it } from 'vitest'

// The source of the app, without tests, by path from src: "features/app/Dashboard.tsx".
const SOURCES: Record<string, string> = Object.fromEntries(
  Object.entries(
    import.meta.glob<string>(['../../**/*.{ts,tsx}', '!../../**/__tests__/**'], {
      eager: true,
      query: '?raw',
      import: 'default',
    }),
  ).map(([file, text]) => [srcPath(file), text]),
)

// Vite names files by their path from this test: "../x.ts" in lib, "../../features/x.tsx".
function srcPath(file: string): string {
  return file.startsWith('../../') ? file.slice('../../'.length) : `lib/${file.slice(3)}`
}

// Libraries the dashboard must load only when a view needs them, not on the first screen.
const HEAVY = /^(monaco-editor|react-markdown|remark-gfm|@xterm\/)/

// Static imports and re-exports. `import type` is left out, and so is `import()`, which is
// what keeps a module out of the first load.
const STATIC_IMPORT = /^(?:import|export)\s+(?!type\b)(?:[^'";]*?\s+from\s+)?'([^']+)'/gm

function withoutComments(text: string): string {
  return text
    .replace(/\/\*[\s\S]*?\*\//g, '')
    .split('\n')
    .filter((line) => !line.trim().startsWith('//'))
    .join('\n')
}

function staticImports(text: string): string[] {
  return [...withoutComments(text).matchAll(STATIC_IMPORT)].map((match) => match[1] ?? '')
}

function packageOf(specifier: string): string {
  const parts = specifier.split('/')
  return specifier.startsWith('@') ? parts.slice(0, 2).join('/') : (parts[0] ?? specifier)
}

function resolveFile(from: string, specifier: string): string | null {
  const parts = from.split('/').slice(0, -1)
  for (const part of specifier.split('?')[0]?.split('/') ?? []) {
    if (part === '..') {
      parts.pop()
    } else if (part !== '.') {
      parts.push(part)
    }
  }
  const base = parts.join('/')
  const found = [base, `${base}.ts`, `${base}.tsx`, `${base}/index.ts`, `${base}/index.tsx`].find(
    (candidate) => SOURCES[candidate] !== undefined,
  )
  return found ?? null
}

// What the browser loads before the dashboard can draw: every file reached from main.tsx by
// static imports, and the libraries they import.
function firstLoad(): { files: Set<string>; packages: Set<string> } {
  const files = new Set<string>()
  const packages = new Set<string>()
  const queue = ['main.tsx']
  while (queue.length > 0) {
    const file = queue.pop() ?? ''
    if (files.has(file)) {
      continue
    }
    files.add(file)
    for (const specifier of staticImports(SOURCES[file] ?? '')) {
      if (specifier.startsWith('.')) {
        const next = resolveFile(file, specifier)
        if (next !== null) {
          queue.push(next)
        }
      } else if (!specifier.startsWith('/')) {
        packages.add(packageOf(specifier))
      }
    }
  }
  return { files, packages }
}

describe('the first load of the dashboard', () => {
  it('reaches the dashboard and its usual libraries', () => {
    const { files, packages } = firstLoad()
    expect(files).toContain('features/app/Dashboard.tsx')
    expect(files).toContain('features/tasks/TaskTable.tsx')
    expect(packages).toContain('react')
    expect(packages).toContain('@tanstack/react-query')
  })

  it('finds the heavy libraries in the source, so the next test means something', () => {
    const all = new Set(Object.values(SOURCES).flatMap(staticImports).map(packageOf))
    for (const name of ['monaco-editor', 'react-markdown', 'remark-gfm', '@xterm/xterm']) {
      expect(all).toContain(name)
    }
  })

  it('does not load the editor, the terminal or the markdown parser', () => {
    expect([...firstLoad().packages].filter((name) => HEAVY.test(name))).toEqual([])
  })

  it('counts the search, the board and the progress view in the first load', () => {
    const { files } = firstLoad()
    for (const file of [
      'features/palette/CommandPalette.tsx',
      'features/tasks/TaskBoard.tsx',
      'features/stages/ProgressBoard.tsx',
    ]) {
      expect(files).toContain(file)
    }
  })

  it('keeps the lazy views out of the first load', () => {
    const { files } = firstLoad()
    for (const file of [
      'features/terminals/TerminalView.tsx',
      'features/git/MonacoDiff.tsx',
      'features/files/MonacoFile.tsx',
      'components/MarkdownBody.tsx',
      'features/files/monaco-setup.ts',
      'features/terminals/session/index.ts',
    ]) {
      expect(files).not.toContain(file)
    }
  })

  // The panes and the dashboard use the cache, so it must not import xterm (see the test above).
  it('counts the terminal cache and the prefetch in the first load', () => {
    const { files } = firstLoad()
    expect(files).toContain('features/terminals/terminal-cache.ts')
    expect(files).toContain('lib/prefetch.ts')
  })

  it('prefetches the diff when idle, so the first diff does not wait for Monaco', () => {
    expect(SOURCES['features/app/useDashboardQueries.ts']).toContain(
      "prefetchWhenIdle(() => import('../git/MonacoDiff'))",
    )
  })

  it('skips type imports and dynamic imports', () => {
    expect(
      staticImports(
        [
          "import type { ITheme } from '@xterm/xterm'",
          "const View = lazy(() => import('./TerminalView'))",
          'import {',
          '  a,',
          '  b,',
          "} from './many'",
          "import './styles.css'",
          "export { c } from './again'",
          "// import { d } from 'commented'",
        ].join('\n'),
      ),
    ).toEqual(['./many', './styles.css', './again'])
  })
})
