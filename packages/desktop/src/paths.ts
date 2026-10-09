import { createRequire } from 'node:module'
import { dirname, join } from 'node:path'

const require = createRequire(import.meta.url)

// Where the parts of Helmsman are: the daemon's entry, the CLI's entry and the built dashboard.
// They are packages of this workspace, found the way Node.js finds any dependency.
export type HelmsmanParts = { daemonMain: string; cliMain: string; webDir: string }

export function findParts(env: NodeJS.ProcessEnv): HelmsmanParts {
  const packageDir = (name: string) => dirname(require.resolve(`${name}/package.json`))
  return {
    // The daemon's exports hide its package.json; its main module sits next to the entry.
    daemonMain: join(dirname(require.resolve('@helmsman/daemon')), 'main.js'),
    cliMain: join(packageDir('@helmsman/cli'), 'dist', 'main.js'),
    webDir: env.HELMSMAN_WEB_DIR?.trim() || join(packageDir('@helmsman/web'), 'dist'),
  }
}
