import { describe, expect, it } from 'vitest'
import { iconUrl } from '../components/FileIcon'
import { fileIconName, namedIcons } from './file-icons'

describe('fileIconName', () => {
  it.each([
    ['src/app.ts', 'typescript'],
    ['src/App.tsx', 'react_ts'],
    ['src/app.test.ts', 'test-ts'],
    ['src/App.test.tsx', 'test-jsx'],
    ['tests/e2e/test.spec.js', 'test-js'],
    ['types/env.d.ts', 'typescript-def'],
    ['lib.mjs', 'javascript'],
    ['package.json', 'nodejs'],
    ['packages/web/package.json', 'nodejs'],
    ['config.json', 'json'],
    ['tsconfig.build.json', 'tsconfig'],
    ['vite.config.ts', 'vite'],
    ['Dockerfile', 'docker'],
    ['docker/prod.Dockerfile', 'docker'],
    ['.env.local', 'tune'],
    ['README.md', 'readme'],
    ['docs/plan.md', 'markdown'],
    ['scripts/dev.sh', 'console'],
    ['logo.PNG', 'image'],
    ['Vagrantfile', 'vagrant'],
    ['disc.yml', 'yaml'],
    ['deploy.sh.bak', 'file'],
    ['.gitignore', 'git'],
    ['Makefile', 'makefile'],
    ['no-extension', 'file'],
    ['.hidden', 'file'],
  ])('%s gets %s', (path, icon) => {
    expect(fileIconName(path)).toBe(icon)
  })
})

describe('the bundled icons', () => {
  it('has every icon the maps name', () => {
    const missing = namedIcons().filter((name) => iconUrl(name, 'dark') === undefined)
    expect(missing).toEqual([])
  })

  it('takes the light version when there is one', () => {
    expect(iconUrl('pnpm', 'light')).toContain('pnpm_light')
    expect(iconUrl('pnpm', 'dark')).not.toContain('_light')
    expect(iconUrl('typescript', 'light')).toBe(iconUrl('typescript', 'dark'))
  })
})
