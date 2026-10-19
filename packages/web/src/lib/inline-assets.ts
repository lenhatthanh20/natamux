// Vite puts small files into the bundle as data addresses. The file icons are many and only a few
// show at a time, so they stay files: the browser loads one when it is drawn, then caches it.
// Fonts stay files too: the content security policy has no font-src, so it blocks data fonts.
export function inlineAsset(file: string): boolean | undefined {
  const path = file.replaceAll('\\', '/')
  return path.includes('/material-icon-theme/') || /\.(woff2?|ttf)$/.test(path) ? false : undefined
}
