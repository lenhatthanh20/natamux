import ReactMarkdown, { type Components } from 'react-markdown'
import remarkGfm from 'remark-gfm'

const remarkPlugins = [remarkGfm]

// Same rule as the desktop window: only web and mail addresses open, so a file cannot start a
// program through another scheme.
function isSafeLink(href: string | undefined): href is string {
  try {
    return href !== undefined && ['http:', 'https:', 'mailto:'].includes(new URL(href).protocol)
  } catch {
    return false
  }
}

const components: Components = {
  a: ({ href, children }) =>
    isSafeLink(href) ? (
      <a href={href} target="_blank" rel="noopener noreferrer">
        {children}
      </a>
    ) : (
      <span>{children}</span>
    ),
  // An image is never loaded: a remote one would tell its server that the file was read.
  img: ({ alt }) => (alt ? <span className="text-muted">{alt}</span> : null),
}

// Markdown shown as text, never as HTML: HTML in the source is dropped.
export function Markdown({ text }: { text: string }) {
  return (
    <div className="hm-markdown">
      <ReactMarkdown remarkPlugins={remarkPlugins} components={components} skipHtml>
        {text}
      </ReactMarkdown>
    </div>
  )
}
