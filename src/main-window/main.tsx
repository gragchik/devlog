import React from 'react'
import { createRoot } from 'react-dom/client'
import { App } from './App'
import '../shared/styles/global.css'

const container = document.getElementById('root')
if (!container) {
  throw new Error('#root не найден в index.html')
}

createRoot(container).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
)
