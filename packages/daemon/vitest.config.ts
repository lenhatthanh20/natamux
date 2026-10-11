import { configDefaults, defineConfig } from 'vitest/config'

// Worktrees of tasks live in .natamux. They hold copies of the tests.
// Many tests run git and shells, which start slowly on the macOS and Windows runners of CI.
export default defineConfig({
  test: {
    exclude: [...configDefaults.exclude, '**/.natamux/**'],
    testTimeout: process.platform === 'linux' ? 5_000 : 30_000,
  },
})
