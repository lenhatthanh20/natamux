// Vite puts small files into the bundle as data addresses. The file icons are many and only a few
// show at a time, so they stay files: the browser loads one when it is drawn, then caches it.
export function inlineAsset(file: string): boolean | undefined {
  return file.replaceAll('\\', '/').includes('/material-icon-theme/') ? false : undefined
}
