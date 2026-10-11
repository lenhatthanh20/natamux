import { lazy } from 'react'
import { LazyView } from './ui/LazyView'

// The markdown parser is a large part of the bundle, and only the Docs tab needs it.
const loadBody = () => import('./MarkdownBody')
const MarkdownBody = lazy(loadBody)

// Starts the download early, so the parser is there by the time the file text arrives.
export function preloadMarkdown(): void {
  loadBody().catch(() => undefined)
}

// Markdown shown as text, never as HTML: HTML in the source is dropped.
export function Markdown({ text }: { text: string }) {
  return (
    <div className="hm-markdown">
      <LazyView label="file view" fallback={null}>
        <MarkdownBody text={text} />
      </LazyView>
    </div>
  )
}
