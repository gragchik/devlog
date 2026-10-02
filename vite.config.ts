import { resolve } from 'node:path'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

// https://v2.tauri.app/start/frontend/vite/
export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      '@shared': resolve(__dirname, 'src/shared')
    }
  },
  // Не даём Vite чистить консоль — иначе не видно ошибок cargo/rust (рекомендация Tauri).
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      // Не триггерить перезапуск Vite на изменения в Rust-дереве — его пересобирает сам Tauri CLI.
      ignored: ['**/src-tauri/**']
    }
  },
  build: {
    rollupOptions: {
      input: {
        main: resolve(__dirname, 'index.html'),
        overlay: resolve(__dirname, 'overlay.html')
      }
    }
  }
})
