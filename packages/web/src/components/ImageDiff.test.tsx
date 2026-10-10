import type { ChangeStatus, ImageSide } from '@natamux/core'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError } from '../lib/api'
import { formatBytes, ImageDiff, type LoadImage } from './ImageDiff'

const created: string[] = []
const revoked: string[] = []
const original = { create: URL.createObjectURL, revoke: URL.revokeObjectURL }

beforeEach(() => {
  created.length = 0
  revoked.length = 0
  // jsdom has no object URLs.
  URL.createObjectURL = vi.fn(() => {
    const url = `blob:test/${created.length + 1}`
    created.push(url)
    return url
  })
  URL.revokeObjectURL = vi.fn((url: string) => {
    revoked.push(url)
  })
})

afterEach(() => {
  cleanup()
  URL.createObjectURL = original.create
  URL.revokeObjectURL = original.revoke
})

function show(status: ChangeStatus, loadImage: LoadImage) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  return render(
    <QueryClientProvider client={client}>
      <ImageDiff
        taskId="T-0001"
        path="assets/logo.png"
        status={status}
        layout="side"
        loadImage={loadImage}
      />
    </QueryClientProvider>,
  )
}

// Two bytes before, three after.
const bytes = async (_path: string, side: ImageSide) =>
  new Blob([side === 'original' ? 'ab' : 'abc'])

describe('ImageDiff', () => {
  it('shows only the new image for an added file', async () => {
    const loadImage = vi.fn(bytes)
    show('A', loadImage)

    expect(await screen.findByRole('img', { name: 'After: assets/logo.png' })).toBeTruthy()
    expect(screen.queryByText('Before (last commit)')).toBeNull()
    expect(loadImage).toHaveBeenCalledWith('assets/logo.png', 'modified')
    expect(loadImage).toHaveBeenCalledTimes(1)
  })

  it('shows only the old image for a deleted file', async () => {
    show('D', bytes)

    expect(await screen.findByRole('img', { name: 'Before: assets/logo.png' })).toBeTruthy()
    expect(screen.queryByText('After (working tree)')).toBeNull()
  })

  it('shows both images for a modified file, with their sizes', async () => {
    show('M', bytes)

    expect(await screen.findByRole('img', { name: 'Before: assets/logo.png' })).toBeTruthy()
    expect(await screen.findByRole('img', { name: 'After: assets/logo.png' })).toBeTruthy()
    expect(screen.getByText('Before (last commit)')).toBeTruthy()
    expect(screen.getByText('After (working tree)')).toBeTruthy()
    expect(screen.getByText('2 B')).toBeTruthy()
    expect(screen.getByText('3 B')).toBeTruthy()
  })

  it('gives the object URLs back when it goes away', async () => {
    const view = show('M', bytes)
    await screen.findByRole('img', { name: 'Before: assets/logo.png' })
    await screen.findByRole('img', { name: 'After: assets/logo.png' })
    expect(created).toHaveLength(2)

    view.unmount()

    expect([...revoked].sort()).toEqual([...created].sort())
  })

  it('says why an image is not shown', async () => {
    show('U', async () => {
      throw new ApiError(413, 'too large')
    })

    expect((await screen.findByRole('alert')).textContent).toBe(
      'This image is larger than 5 MB, so it is not shown.',
    )
  })

  it('shows the message of any other error', async () => {
    show('U', async () => {
      throw new Error('The daemon answered with status 500.')
    })

    expect((await screen.findByRole('alert')).textContent).toBe(
      'The daemon answered with status 500.',
    )
  })
})

describe('formatBytes', () => {
  it('gives bytes, kilobytes and megabytes', () => {
    expect(formatBytes(512)).toBe('512 B')
    expect(formatBytes(12_300)).toBe('12.3 KB')
    expect(formatBytes(4_200_000)).toBe('4.2 MB')
  })
})
