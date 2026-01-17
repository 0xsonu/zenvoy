import React from 'react'
import ReactDOM from 'react-dom/client'
import './styles/index.css'

function App() {
  return (
    <div className="h-screen w-screen flex items-center justify-center bg-zen-bg text-zen-fg">
      <h1 className="text-2xl font-semibold">Zenvoy</h1>
    </div>
  )
}

const root = document.getElementById('root')
if (root) {
  ReactDOM.createRoot(root).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>
  )
}
