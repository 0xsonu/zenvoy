import React, { Suspense } from 'react'
import ReactDOM from 'react-dom/client'
import { createTauriBridge } from './bridge/tauri-bridge'
import { installZenBridge } from './bridge/contract'
import './app/styles/index.css'

const App = React.lazy(() => import('./app/App'))

const root = document.getElementById('root')

function renderBootError(message: string): void {
  if (!root) return
  root.replaceChildren()
  const pre = document.createElement('pre')
  pre.style.padding = '24px'
  pre.style.color = '#b42318'
  pre.style.background = '#fff7f7'
  pre.style.font = '14px/1.5 ui-monospace, SFMono-Regular, Menlo, monospace'
  pre.style.whiteSpace = 'pre-wrap'
  pre.textContent = message
  root.appendChild(pre)
}

let booted = false

window.addEventListener('error', (event) => {
  console.error('[zenvoy] uncaught error', event.error ?? event.message)
  if (!booted) renderBootError(String(event.error?.stack ?? event.error ?? event.message))
})

window.addEventListener('unhandledrejection', (event) => {
  console.error('[zenvoy] unhandled rejection', event.reason)
  if (!booted) renderBootError(String(event.reason?.stack ?? event.reason))
})

try {
  if (!root) {
    throw new Error('Root element #root was not found')
  }

  const bridge = createTauriBridge()
  installZenBridge(bridge)

  const params = new URLSearchParams(window.location.search)
  const isFloating = params.get('floating') === '1'
  const isQuickCapture = params.get('quickCapture') === '1'
  const isExternalFile = params.get('externalFile') !== null
  const floatingNotePath = params.get('note')

  // TODO: render specialized windows for floating/quickCapture/externalFile
  void isFloating
  void isQuickCapture
  void isExternalFile
  void floatingNotePath

  ReactDOM.createRoot(root).render(
    <React.StrictMode>
      <Suspense fallback={null}>
        <App />
      </Suspense>
    </React.StrictMode>
  )

  booted = true
} catch (error) {
  console.error('[zenvoy] boot failed', error)
  renderBootError(String(error instanceof Error ? error.stack ?? error.message : error))
}
