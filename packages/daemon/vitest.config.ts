import { configDefaults, defineConfig } from 'vitest/config'

// Worktrees of tasks live in .helmsman. They hold copies of the tests.
export default defineConfig({
  test: { exclude: [...configDefaults.exclude, '**/.helmsman/**'] },
})
