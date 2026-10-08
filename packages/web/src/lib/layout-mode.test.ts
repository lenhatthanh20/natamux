import { describe, expect, it } from 'vitest'
import { decideLayout, effectiveSidebarWidth } from './layout-mode'

const base = { sidebarWidth: 272, detailsWidth: 360, coarse: false }

describe('decideLayout', () => {
  it('uses the desktop layout when the width is unknown', () => {
    expect(decideLayout({ ...base, width: null })).toEqual({
      sidebar: 'docked',
      details: 'docked',
      coarse: false,
      compact: false,
    })
  })

  it('docks both panels at 1440 and above', () => {
    const layout = decideLayout({ ...base, width: 1440 })
    expect(layout.sidebar).toBe('docked')
    expect(layout.details).toBe('docked')
  })

  it('docks the details at 1280 only while the main area keeps 640', () => {
    expect(decideLayout({ ...base, width: 1280 }).details).toBe('docked')
    expect(decideLayout({ ...base, width: 1280, sidebarWidth: 320 }).details).toBe('sheet')
  })

  it('counts a collapsed sidebar as no width', () => {
    expect(decideLayout({ ...base, width: 1280, sidebarWidth: 0 }).details).toBe('docked')
  })

  it('makes the details a sheet from 768 to 1279', () => {
    expect(decideLayout({ ...base, width: 1279 }).details).toBe('sheet')
    expect(decideLayout({ ...base, width: 1024 }).details).toBe('sheet')
    expect(decideLayout({ ...base, width: 768 }).details).toBe('sheet')
  })

  it('makes the sidebar a drawer under 1024', () => {
    expect(decideLayout({ ...base, width: 1024 }).sidebar).toBe('docked')
    expect(decideLayout({ ...base, width: 1023 }).sidebar).toBe('drawer')
    expect(decideLayout({ ...base, width: 1023 }).compact).toBe(true)
  })

  it('makes the details a page under 768', () => {
    expect(decideLayout({ ...base, width: 767 }).details).toBe('page')
    expect(decideLayout({ ...base, width: 390 }).details).toBe('page')
  })

  it('passes the pointer type through', () => {
    expect(decideLayout({ ...base, width: 1440, coarse: true }).coarse).toBe(true)
  })
})

describe('effectiveSidebarWidth', () => {
  it('limits the sidebar to 30% of the window from 1024 to 1279', () => {
    expect(effectiveSidebarWidth(272, 1024)).toBe(272)
    expect(effectiveSidebarWidth(440, 1100)).toBe(330)
  })

  it('leaves the width alone elsewhere', () => {
    expect(effectiveSidebarWidth(440, 1280)).toBe(440)
    expect(effectiveSidebarWidth(440, null)).toBe(440)
  })
})
