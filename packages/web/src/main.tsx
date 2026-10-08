import '@fontsource-variable/geist'
import '@fontsource-variable/geist-mono'
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { App } from './App'
import { startTheme } from './lib/theme'
import './styles.css'

startTheme()

const root = document.getElementById('root')
if (!root) {
  throw new Error('The page has no #root element.')
}

createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
