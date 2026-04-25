import { useEffect, useState } from 'react'

function App(): JSX.Element {
  const [ready, setReady] = useState(false)

  useEffect(() => {
    // Prove the bridge is installed and callable
    const info = window.zen.getAppInfo()
    console.log('[zenvoy] bridge active, runtime:', info.runtime)
    setReady(true)
  }, [])

  if (!ready) {
    return (
      <div className="flex h-screen w-screen items-center justify-center bg-paper-100 text-ink-900">
        <span className="text-sm text-ink-500">Loading…</span>
      </div>
    )
  }

  return (
    <div className="flex h-screen w-screen flex-col bg-paper-100 text-ink-900">
      {/* Title bar area */}
      <header className="flex h-10 shrink-0 items-center border-b border-paper-200 px-4" data-tauri-drag-region="">
        <span className="text-sm font-medium">Zenvoy</span>
      </header>

      {/* Main content shell: sidebar + editor placeholder */}
      <div className="flex min-h-0 flex-1">
        <aside className="w-56 shrink-0 border-r border-paper-200 p-3">
          <p className="text-xs text-ink-500">Sidebar (TODO)</p>
        </aside>
        <main className="flex flex-1 items-center justify-center">
          <p className="text-ink-600">Editor pane (TODO)</p>
        </main>
      </div>
    </div>
  )
}

export default App
