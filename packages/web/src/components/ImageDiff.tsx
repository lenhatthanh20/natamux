import type { ChangeStatus, ImageSide } from '@natamux/core'
import { useQuery } from '@tanstack/react-query'
import { Fragment, useEffect, useState } from 'react'
import { ApiError } from '../lib/api'

export type LoadImage = (path: string, side: ImageSide) => Promise<Blob>

type ImageDiffProps = {
  taskId: string
  path: string
  status: ChangeStatus
  layout: 'side' | 'stacked'
  loadImage: LoadImage
}

const CAPTION: Record<ImageSide, string> = {
  original: 'Before (last commit)',
  modified: 'After (working tree)',
}
const SHORT: Record<ImageSide, string> = { original: 'Before', modified: 'After' }

// A new file has no before, a deleted one has no after.
function sidesOf(status: ChangeStatus): ImageSide[] {
  if (status === 'A' || status === 'U') {
    return ['modified']
  }
  return status === 'D' ? ['original'] : ['original', 'modified']
}

export function formatBytes(bytes: number): string {
  if (bytes < 1000) {
    return `${bytes} B`
  }
  if (bytes < 1_000_000) {
    return `${(bytes / 1000).toFixed(1)} KB`
  }
  return `${(bytes / 1_000_000).toFixed(1)} MB`
}

// An address for the bytes, given back when they are replaced or the panel goes away.
export function useObjectUrl(blob: Blob | undefined): string | null {
  const [url, setUrl] = useState<string | null>(null)
  useEffect(() => {
    if (blob === undefined) {
      setUrl(null)
      return
    }
    const next = URL.createObjectURL(blob)
    setUrl(next)
    return () => URL.revokeObjectURL(next)
  }, [blob])
  return url
}

function errorText(error: Error): string {
  return error instanceof ApiError && error.status === 413
    ? 'This image is larger than 5 MB, so it is not shown.'
    : error.message
}

type NaturalSize = { url: string; width: number; height: number }

function ImagePanel({
  taskId,
  path,
  status,
  side,
  loadImage,
}: Omit<ImageDiffProps, 'layout'> & { side: ImageSide }) {
  // Under 'file-diff', so a refresh of the changes reads the image again too.
  const image = useQuery({
    queryKey: ['file-diff', taskId, path, status, 'image', side],
    queryFn: () => loadImage(path, side),
    refetchInterval: false,
  })
  const url = useObjectUrl(image.data)
  // Kept with its address, so a new image does not show the size of the one before it.
  const [natural, setNatural] = useState<NaturalSize | null>(null)
  const facts = [
    CAPTION[side],
    natural === null || natural.url !== url ? null : `${natural.width} × ${natural.height} px`,
    image.data === undefined ? null : formatBytes(image.data.size),
  ].filter((fact) => fact !== null)

  return (
    <figure className="flex min-w-0 flex-col gap-2">
      <figcaption className="text-xs text-muted">
        <span className="font-medium text-ink-2">{facts[0]}</span>
        {facts.slice(1).map((fact) => (
          <Fragment key={fact}>
            {' · '}
            <span className="tabular-nums">{fact}</span>
          </Fragment>
        ))}
      </figcaption>
      {image.isError ? (
        <p role="alert" className="p-2">
          {errorText(image.error)}
        </p>
      ) : url === null ? (
        <div role="status" aria-label={`Loading the ${SHORT[side].toLowerCase()} image`}>
          <span aria-hidden="true" className="hm-skeleton hm-pulse block h-40 rounded-md" />
        </div>
      ) : (
        <div className="hm-checker flex min-h-24 items-center justify-center p-2">
          <img
            key={url}
            src={url}
            alt={`${SHORT[side]}: ${path}`}
            onLoad={(event) =>
              setNatural({
                url,
                width: event.currentTarget.naturalWidth,
                height: event.currentTarget.naturalHeight,
              })
            }
            className="max-h-[60vh] max-w-full object-contain"
          />
        </div>
      )}
    </figure>
  )
}

// A changed image, before and after, on a checkerboard so that transparent parts show.
export function ImageDiff({ taskId, path, status, layout, loadImage }: ImageDiffProps) {
  const sides = sidesOf(status)
  const shape =
    layout === 'side' && sides.length === 2 ? 'grid grid-cols-2 content-start' : 'flex flex-col'
  return (
    <section
      aria-label={`Changes in ${path}`}
      className={`min-h-0 flex-1 gap-3 overflow-auto p-3 ${shape}`}
    >
      {sides.map((side) => (
        <ImagePanel
          key={side}
          taskId={taskId}
          path={path}
          status={status}
          side={side}
          loadImage={loadImage}
        />
      ))}
    </section>
  )
}
