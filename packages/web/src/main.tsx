import '@fontsource-variable/geist'
import '@fontsource-variable/geist-mono'
import '@fontsource-variable/inter'
import '@fontsource-variable/inter/wght-italic.css'
import '@fontsource-variable/jetbrains-mono'
import '@fontsource-variable/jetbrains-mono/wght-italic.css'
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { App } from './App'
import { startMotionTicker } from './lib/motion-ticker'
import { startRouting } from './lib/route-sync'
import { startTheme } from './lib/theme'
import { startUiSize } from './lib/ui-size'
import { startWindowFocus } from './lib/window-focus'
import './styles.css'

startTheme()
startUiSize()
startWindowFocus()
startMotionTicker()
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
