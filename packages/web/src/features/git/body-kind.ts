import { type FileDiff, imageTypeOf } from '@natamux/core'
import { wholeFileKind } from '../../lib/changes'
import type { BodyKind } from './DiffToolbar'

// An image that cannot be shown as text, or an SVG that the reader wants to see drawn.
export function showsImage(file: FileDiff, svgPreview: boolean): boolean {
  return imageTypeOf(file.path) !== null && (file.binary || file.tooLarge || svgPreview)
}

// Must match what FileBody shows, so the toolbar has no buttons that do nothing.
export function bodyKindOf(
  file: FileDiff | undefined,
  svgPreview: boolean,
  sameText: boolean,
): BodyKind {
  if (file === undefined) {
    return 'pending'
  }
  if (showsImage(file, svgPreview)) {
    return 'image'
  }
  if (file.binary || file.tooLarge) {
    return 'notice'
  }
  const kind = wholeFileKind(file.status)
  if (kind !== null) {
    return wholeText(file, kind) === '' ? 'notice' : 'whole'
  }
  return sameText ? 'notice' : 'text'
}

export function wholeText(file: FileDiff, kind: 'added' | 'removed'): string {
  return (kind === 'added' ? file.modified : file.original) ?? ''
}
