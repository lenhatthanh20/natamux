import '@fontsource-variable/geist'
import '@fontsource-variable/geist-mono'
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { App } from './App'
import { startRouting } from './lib/route-sync'
import { startTheme } from './lib/theme'
import { startUiSize } from './lib/ui-size'
import './styles.css'

startTheme()
startUiSize()
startRouting()

const root = document.getElementById('root')
if (!root) {
  throw new Error('The page has no #root element.')
}

createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
