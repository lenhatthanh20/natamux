// macOS 26 gives role items (About, Hide, Quit…) an icon and indents every title in a menu section
// that has one, so our items need icons there too. Older macOS has no icons to match.
export function menuIconsOn(platform: NodeJS.Platform, systemVersion: string): boolean {
  return platform === 'darwin' && Number(systemVersion.split('.')[0]) >= 26
}
