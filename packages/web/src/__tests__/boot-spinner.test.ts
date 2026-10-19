import { describe, expect, it } from 'vitest'
import page from '../../index.html?raw'

function parse(): Document {
  return new DOMParser().parseFromString(page, 'text/html')
}

function css(): string {
  return parse().querySelector('style')?.textContent ?? ''
}

describe('the spinner of index.html', () => {
  // The desktop smoke check reads a child of #root as "the dashboard rendered".
  it('sits after #root, not inside it', () => {
    const root = parse().getElementById('root')
    expect(root?.childElementCount).toBe(0)
    expect(root?.nextElementSibling?.id).toBe('boot')
  })

  it('hides once React draws into #root', () => {
    expect(css()).toContain('#root:not(:empty)+#boot{display:none}')
    const doc = parse()
    expect(doc.querySelector('#root:not(:empty)+#boot')).toBeNull()
    doc.getElementById('root')?.append(doc.createElement('div'))
    expect(doc.querySelector('#root:not(:empty)+#boot')).not.toBeNull()
  })

  // styles.css paints the body by data-theme, which main.tsx sets before the first render.
  it('paints the page only until the theme is set', () => {
    expect(css()).toContain(':root:not([data-theme]) body{')
    expect(css()).not.toMatch(/(^|})\s*body\s*\{/)
  })

  it('shows still dots instead of turning dashes with reduced motion', () => {
    const reduced = css().slice(css().indexOf('@media (prefers-reduced-motion:reduce)'))
    expect(reduced).toContain('animation:none')
    expect(reduced).toContain('.boot-dots{display:inline}')
  })

  // The page's content security policy runs scripts from its own origin only.
  it('adds no inline script', () => {
    for (const script of parse().querySelectorAll('script')) {
      expect(script.getAttribute('src')).toBeTruthy()
    }
  })
})
