import { Component, type ReactNode, Suspense } from 'react'
import { Button } from './Button'

type LazyViewProps = {
  // What failed to load, in the error: "the terminal".
  label: string
  fallback: ReactNode
  children: ReactNode
}

// A view whose code loads on demand. When that code cannot load (offline, or the app was updated
// and the old files are gone), the error stays in this view and the rest of the page keeps working.
// React keeps a failed import, so only a reload tries again.
export class LazyView extends Component<LazyViewProps, { error: Error | null }> {
  override state: { error: Error | null } = { error: null }

  static getDerivedStateFromError(error: unknown): { error: Error } {
    return { error: error instanceof Error ? error : new Error(String(error)) }
  }

  override render() {
    const { label, fallback, children } = this.props
    const { error } = this.state
    if (error !== null) {
      return (
        <div role="alert" className="flex flex-wrap items-center gap-3 p-4">
          <span className="min-w-0 flex-1">
            Could not load the {label}. {error.message}
          </span>
          <Button size="sm" onClick={() => window.location.reload()}>
            Reload
          </Button>
        </div>
      )
    }
    return <Suspense fallback={fallback}>{children}</Suspense>
  }
}
