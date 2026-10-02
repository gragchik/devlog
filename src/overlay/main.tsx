import React from 'react'
import { createRoot } from 'react-dom/client'
import { OverlayApp } from './OverlayApp'
import '../shared/styles/global.css'

const container = document.getElementById('root')
if (!container) {
  throw new Error('#root не найден в overlay.html')
}

createRoot(container).render(
  <React.StrictMode>
    <OverlayApp />
  </React.StrictMode>
)
