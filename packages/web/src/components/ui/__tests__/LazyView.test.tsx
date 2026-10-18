import { cleanup, render, screen } from '@testing-library/react'
import { lazy } from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { LazyView } from '../LazyView'

afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
})

describe('LazyView', () => {
  it('shows the fallback, then the view once its code has loaded', async () => {
    const View = lazy(async () => ({ default: () => <p>The view</p> }))
    render(
      <LazyView label="viewer" fallback={<p>Loading</p>}>
        <View />
      </LazyView>,
    )

    expect(screen.getByText('Loading')).toBeTruthy()
    expect(await screen.findByText('The view')).toBeTruthy()
  })

  it('shows an error in its place when the code cannot load, and the rest of the page stays', async () => {
    // React reports the caught error on the console.
    vi.spyOn(console, 'error').mockImplementation(() => undefined)
    const Broken = lazy(() => Promise.reject(new Error('Failed to fetch')))
    render(
      <>
        <p>The task list</p>
        <LazyView label="terminal" fallback={null}>
          <Broken />
        </LazyView>
      </>,
    )

    const alert = await screen.findByRole('alert')
    expect(alert.textContent).toContain('Could not load the terminal.')
    expect(alert.textContent).toContain('Failed to fetch')
    expect(screen.getByRole('button', { name: 'Reload' })).toBeTruthy()
    expect(screen.getByText('The task list')).toBeTruthy()
  })
})
