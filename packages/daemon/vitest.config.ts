import { configDefaults, defineConfig } from 'vitest/config'

// Worktrees of tasks live in .natamux. They hold copies of the tests.
export default defineConfig({
  test: { exclude: [...configDefaults.exclude, '**/.natamux/**'] },
})
