import tailwindcss from '@tailwindcss/vite'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vitest/config'

// The dashboard talks to the daemon through /api, so the browser never needs the daemon's address.
// In Docker the daemon is another container, named in NATAMUX_DAEMON_URL.
const daemonUrl = process.env.NATAMUX_DAEMON_URL ?? 'http://127.0.0.1:4317'

export default defineConfig({
  plugins: [react(), tailwindcss()],
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
  },
})
