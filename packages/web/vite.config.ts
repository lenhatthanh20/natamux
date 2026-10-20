import tailwindcss from '@tailwindcss/vite'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vitest/config'
import { inlineAsset } from './src/lib/inline-assets'

// The dashboard talks to the daemon through /api, so the browser never needs the daemon's address.
// In Docker the daemon is another container, named in NATAMUX_DAEMON_URL.
const daemonUrl = process.env.NATAMUX_DAEMON_URL ?? 'http://127.0.0.1:4317'

export default defineConfig({
  plugins: [react(), tailwindcss()],
  build: {
    assetsInlineLimit: inlineAsset,
  },
  server: {
    proxy: {
      '/api': {
        target: daemonUrl,
        ws: true,
        rewrite: (path) => path.replace(/^\/api/, ''),
      },
    },
  },
  test: {
    environment: 'jsdom',
    // The longest App tests walk through a whole view and take over the 5 s default now and then
    // on slow runners (macOS, Windows). Coverage no longer runs in CI, but they still need this.
    testTimeout: 20_000,
    // Vitest turns CSS into an empty string; a test reads the motion rules of this file.
    css: { include: [/src\/styles\.css/] },
  },
})
