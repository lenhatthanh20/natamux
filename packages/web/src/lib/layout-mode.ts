export type SidebarMode = 'docked' | 'drawer'
export type DetailsMode = 'docked' | 'sheet' | 'page'
export type LayoutMode = {
  sidebar: SidebarMode
  details: DetailsMode
  coarse: boolean
  // Under 1024 a sheet or drawer comes with a scrim; above it the sheet floats without one.
  compact: boolean
}

// The same steps as the Tailwind breakpoints md, lg and xl.
export const BREAKPOINT = { md: 768, lg: 1024, xl: 1280 } as const
export const MIN_MAIN_WIDTH_FOR_DOCKED_DETAILS = 640
// One default for both side panels, so the page looks balanced. At 1280 two of them still
// leave main its 640, so the details stay docked.
export const PANEL_WIDTH = 320

// Without a measured width (tests, old browsers) the page gets the desktop layout.
export const DESKTOP_LAYOUT: LayoutMode = {
  sidebar: 'docked',
  details: 'docked',
  coarse: false,
  compact: false,
}

type LayoutInput = {
  width: number | null
  // The width the docked sidebar takes, 0 when the user collapsed it.
  sidebarWidth: number
  detailsWidth: number
  coarse: boolean
}

export function decideLayout({
  width,
  sidebarWidth,
  detailsWidth,
  coarse,
}: LayoutInput): LayoutMode {
  if (width === null) {
    return { ...DESKTOP_LAYOUT, coarse }
  }
  const sidebar: SidebarMode = width >= BREAKPOINT.lg ? 'docked' : 'drawer'
  let details: DetailsMode =
    width >= BREAKPOINT.xl ? 'docked' : width >= BREAKPOINT.md ? 'sheet' : 'page'
  // A wide sidebar must never crush the main area: then the details float over it.
  if (details === 'docked') {
    const main = width - (sidebar === 'docked' ? sidebarWidth : 0) - detailsWidth
    if (main < MIN_MAIN_WIDTH_FOR_DOCKED_DETAILS) {
      details = 'sheet'
    }
  }
  return { sidebar, details, coarse, compact: width < BREAKPOINT.lg }
}

// In 1024-1279 the sidebar may take at most 30% of the window.
export function effectiveSidebarWidth(stored: number, width: number | null): number {
  if (width !== null && width >= BREAKPOINT.lg && width < BREAKPOINT.xl) {
    return Math.min(stored, Math.round(width * 0.3))
  }
  return stored
}
