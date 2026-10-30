import { svelte } from '@sveltejs/vite-plugin-svelte'
import { defineConfig } from 'vitest/config'

// Tauri expects a fixed dev-server port and does not want the terminal cleared.
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  // Inline (empty) PostCSS config stops Vite from picking up a stray config in a parent dir.
  css: { postcss: {} },
  server: { port: 1420, strictPort: true },
  test: { include: ['src/**/*.test.ts'] },
})
