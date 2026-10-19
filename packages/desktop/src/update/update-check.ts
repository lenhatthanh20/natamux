// The parts of the updater that need no Electron, so tests can run them. See
// docs/design-desktop-update.md.

// GitHub redirects this page to /releases/tag/<tag> of the newest release that is not a draft or
// a pre-release. Unlike the REST API, the web page has no hourly limit per address, which shared
// networks and CI runners reach.
export const LATEST_URL = 'https://github.com/lenhatthanh20/natamux/releases/latest'
export const SUMS_FILE = 'SHA256SUMS.txt'
export const SIGNATURE_FILE = `${SUMS_FILE}.minisig`

export type InstallKind = 'mac' | 'appimage' | 'deb' | 'manual'

export type Release = {
  // As in the address, such as v1.2.3. The signed SHA256SUMS.txt names it.
  tag: string
  version: string
  // The release page.
  page: string
  // The files of the release are at <downloads><name>.
  downloads: string
}

const VERSION = /^v?(\d+)\.(\d+)\.(\d+)$/

export function parseVersion(text: string): [number, number, number] | undefined {
  const match = VERSION.exec(text.trim())
  return match ? [Number(match[1]), Number(match[2]), Number(match[3])] : undefined
}

// Pre-release tags such as 1.2.0-beta.1 never count as newer.
export function isNewer(candidate: string, current: string): boolean {
  const next = parseVersion(candidate)
  const now = parseVersion(current)
  if (next === undefined || now === undefined) {
    return false
  }
  for (let i = 0; i < 3; i++) {
    if (next[i] !== now[i]) {
      return (next[i] ?? 0) > (now[i] ?? 0)
    }
  }
  return false
}

// The release from the address /releases/latest ends at. Undefined when it is not a stable
// release, or when there is no release and GitHub shows the list instead.
export function releaseFromUrl(url: string): Release | undefined {
  const match = /^(https:\/\/github\.com\/[^/]+\/[^/]+)\/releases\/tag\/([^/?#]+)$/.exec(url)
  if (match === null) {
    return undefined
  }
  const [, repo, tag] = match
  const name = decodeURIComponent(tag ?? '')
  const version = parseVersion(name)
  if (repo === undefined || tag === undefined || version === undefined) {
    return undefined
  }
  return {
    tag: name,
    version: version.join('.'),
    page: url,
    downloads: `${repo}/releases/download/${tag}/`,
  }
}

// The Natamux.app folder that holds the running program, from process.execPath.
export function macBundle(execPath: string): string | undefined {
  const at = execPath.indexOf('.app/Contents/MacOS/')
  return at === -1 ? undefined : execPath.slice(0, at + '.app'.length)
}

function parent(path: string): string {
  return path.slice(0, path.lastIndexOf('/')) || '/'
}

export function installKind(input: {
  platform: NodeJS.Platform
  execPath: string
  env: NodeJS.ProcessEnv
  canWrite: (dir: string) => boolean
}): InstallKind {
  const { platform, execPath, env, canWrite } = input
  if (platform === 'darwin') {
    const bundle = macBundle(execPath)
    // An app run from the disk image, or one macOS moved to a random place because it is still
    // quarantined, cannot replace itself.
    if (
      bundle === undefined ||
      bundle.startsWith('/Volumes/') ||
      bundle.includes('/AppTranslocation/')
    ) {
      return 'manual'
    }
    return canWrite(parent(bundle)) && canWrite(bundle) ? 'mac' : 'manual'
  }
  if (platform === 'linux') {
    const appImage = env.APPIMAGE
    if (appImage !== undefined && appImage !== '') {
      return canWrite(parent(appImage)) ? 'appimage' : 'manual'
    }
    if (execPath.startsWith('/opt/Natamux/')) {
      return 'deb'
    }
  }
  return 'manual'
}

// The release file for an install kind. The names come from artifactName in
// electron-builder.yml.
export function assetName(kind: InstallKind, arch: string): string | undefined {
  switch (kind) {
    case 'mac':
      return arch === 'arm64' || arch === 'x64' ? `Natamux-mac-${arch}.dmg` : undefined
    case 'appimage':
      return arch === 'x64' ? 'Natamux-linux-x86_64.AppImage' : undefined
    case 'deb':
      return arch === 'x64' ? 'Natamux-linux-amd64.deb' : undefined
    default:
      return undefined
  }
}

// The hash of one file in a sha256sum list: "<hash>  <name>" or "<hash> *<name>".
export function sha256From(sums: string, name: string): string | undefined {
  for (const line of sums.split('\n')) {
    const match = /^([0-9a-f]{64}) [ *](.+)$/i.exec(line.trim())
    if (match?.[2] === name) {
      return match[1]?.toLowerCase()
    }
  }
  return undefined
}

// Runs detached after the app quits: sh install.sh <pid> <new app> <app> <log> <1 to open it>.
export const MAC_INSTALL_SCRIPT = `#!/bin/sh
pid="$1"; new="$2"; app="$3"; log="$4"; relaunch="$5"
note() { printf '%s install: %s\\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$1" >> "$log"; }
i=0
while kill -0 "$pid" 2>/dev/null; do
  i=$((i + 1))
  if [ "$i" -gt 600 ]; then note "the app did not quit in 60 seconds, the update waits"; exit 1; fi
  sleep 0.1
done
old="$app.old"
rm -rf "$old"
if mv "$app" "$old" && mv "$new" "$app"; then
  rm -rf "$old"
  note "done: $app"
else
  [ -d "$app" ] || mv "$old" "$app"
  note "could not move the new version in, kept the old one"
fi
if [ "$relaunch" = 1 ]; then open "$app"; fi
`
