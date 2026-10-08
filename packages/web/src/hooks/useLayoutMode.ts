import { useSyncExternalStore } from 'react'
import { decideLayout, type LayoutMode } from '../lib/layout-mode'

type Inputs = { sidebarWidth: number; detailsWidth: number }

function canMeasure(): boolean {
  return typeof window !== 'undefined' && typeof window.matchMedia === 'function'
}

function subscribe(onChange: () => void): () => void {
  if (!canMeasure()) {
    return () => undefined
  }
  const coarse = window.matchMedia('(pointer: coarse)')
  window.addEventListener('resize', onChange)
  coarse.addEventListener('change', onChange)
  return () => {
    window.removeEventListener('resize', onChange)
    coarse.removeEventListener('change', onChange)
  }
}

// A number is a snapshot that stays equal between renders, so React does not loop.
function width(): number | null {
  return canMeasure() ? window.innerWidth : null
}

function isCoarse(): boolean {
  return canMeasure() && window.matchMedia('(pointer: coarse)').matches
}

// The window width, or null when it cannot be measured.
export function useViewportWidth(): number | null {
  return useSyncExternalStore(subscribe, width, () => null)
}

// How the shell lays itself out at this window size. See section 3.2 of the UI spec.
export function useLayoutMode({ sidebarWidth, detailsWidth }: Inputs): LayoutMode & {
  width: number | null
} {
  const viewport = useSyncExternalStore(subscribe, width, () => null)
  const coarse = useSyncExternalStore(subscribe, isCoarse, () => false)
  return { ...decideLayout({ width: viewport, sidebarWidth, detailsWidth, coarse }), width: viewport }
}

function isPhone(): boolean {
  return canMeasure() && window.innerWidth < 768
}

// Under 768px. False when the width cannot be measured.
export function usePhone(): boolean {
  return useSyncExternalStore(subscribe, isPhone, () => false)
}
