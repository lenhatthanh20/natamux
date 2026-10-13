import * as nodeModule from 'node:module'

// Runs before main.js loads, because static imports load before the body of a module runs. The
// compile cache then covers all of them. Older runtimes lack it; a cache that cannot be written is
// reported in the return value, so the start never fails on it.
if (typeof nodeModule.enableCompileCache === 'function') {
  nodeModule.enableCompileCache()
}

await import('./main.js')
