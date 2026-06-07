import React, { lazy, Suspense } from 'react'
import ReactDOM from 'react-dom/client'
import { installZenBridge } from './bridge/contract'
import './app/styles/index.css'

const App = lazy(() => import('./app/App'))
const FloatingNoteApp = lazy(() => import('./app/components/FloatingNoteApp').then(m => ({ default: m.FloatingNoteApp })))
const QuickCaptureApp = lazy(() => import('./app/components/QuickCaptureApp').then(m => ({ default: m.QuickCaptureApp })))
const ExternalFileApp = lazy(() => import('./app/components/ExternalFileApp').then(m => ({ default: m.ExternalFileApp })))

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

async function boot() {
  if (!root) throw new Error('Root element #root was not found')

  const isTauri = !!(window as any).__TAURI_INTERNALS__

  if (isTauri) {
    const { createTauriBridge } = await import('./bridge/tauri-bridge')
    installZenBridge(createTauriBridge())
  } else {
    const { httpBridge } = await import('./bridge/http-bridge')
    installZenBridge(httpBridge)
  }

  const params = new URLSearchParams(window.location.search)
  const isFloating = params.get('floating') === '1'
  const isQuickCapture = params.get('quickCapture') === '1'
  const isExternalFile = params.get('externalFile') !== null
  const floatingNotePath = params.get('note')

  ReactDOM.createRoot(root).render(
    <React.StrictMode>
      <Suspense fallback={null}>
        {isQuickCapture ? (
          <QuickCaptureApp />
        ) : isExternalFile ? (
          <ExternalFileApp />
        ) : isFloating && floatingNotePath ? (
          <FloatingNoteApp notePath={floatingNotePath} />
        ) : (
          <App />
        )}
      </Suspense>
    </React.StrictMode>
  )

  booted = true
}

boot().catch((error) => {
  console.error('[zenvoy] boot failed', error)
  renderBootError(String(error instanceof Error ? error.stack ?? error.message : error))
})
