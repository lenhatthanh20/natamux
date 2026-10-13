import { createRequire } from 'node:module'
import { dirname, join } from 'node:path'

const require = createRequire(import.meta.url)

// Where the parts of Natamux are: the daemon's entry, the CLI's entry and the built dashboard.
// They are packages of this workspace, found the way Node.js finds any dependency.
export type NatamuxParts = { daemonMain: string; cliMain: string; webDir: string }

export function findParts(env: NodeJS.ProcessEnv): NatamuxParts {
  const packageDir = (name: string) => dirname(require.resolve(`${name}/package.json`))
  return {
    // The daemon's exports hide its package.json. Its entry, which turns on the compile cache, sits
    // next to index.js.
    daemonMain: join(dirname(require.resolve('@natamux/daemon')), 'entry.js'),
    cliMain: join(packageDir('@natamux/cli'), 'dist', 'main.js'),
    webDir: env.NATAMUX_WEB_DIR?.trim() || join(packageDir('@natamux/web'), 'dist'),
  }
}
