// Marks the page while its window is not focused. The CSS then shows the still look of reduced
// motion: any running animation makes the browser draw every frame, about 20% CPU on a dashboard
// left open on a second screen.
export function startWindowFocus(target: Window = window): () => void {
  const root = target.document.documentElement
  const sync = () => {
    if (target.document.hasFocus()) {
      delete root.dataset.window
    } else {
      root.dataset.window = 'inactive'
    }
  }
  sync()
  target.addEventListener('focus', sync)
  target.addEventListener('blur', sync)
  return () => {
    target.removeEventListener('focus', sync)
    target.removeEventListener('blur', sync)
  }
}
