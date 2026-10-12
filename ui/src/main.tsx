import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'

import './i18n'
import './index.css'
import App from './App.tsx'
import { bootSettings, hydrateSettings } from './state/app'

// an app, not a web page: no browser menu on right click, no browser keys (back, reload, print)
window.addEventListener('contextmenu', (e) => e.preventDefault())
window.addEventListener('keydown', (e) => {
  const k = e.key.toLowerCase()
  const browser = e.key === 'F5' || e.key === 'BrowserBack' || e.key === 'BrowserForward'
    || ((e.ctrlKey || e.metaKey) && (k === 'r' || k === 'p'))
    || (e.altKey && (e.key === 'ArrowLeft' || e.key === 'ArrowRight'))
  if (browser) e.preventDefault()
}, true)

// the interface's choices first (open sections, views), so it opens as it was left
void hydrateSettings().finally(() => {
  void bootSettings()
  createRoot(document.getElementById('root')!).render(
    <StrictMode>
      <App />
    </StrictMode>,
  )
})
