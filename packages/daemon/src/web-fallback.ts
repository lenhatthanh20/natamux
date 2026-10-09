import { existsSync, statSync } from 'node:fs'
import { readFile } from 'node:fs/promises'
import { extname, isAbsolute, join, relative, resolve } from 'node:path'
import type { FastifyInstance, FastifyRequest } from 'fastify'

// The same rules as docker/nginx.conf, for when the daemon serves the dashboard itself.
const PAGE_HEADERS = {
  'x-content-type-options': 'nosniff',
  'x-frame-options': 'DENY',
  'referrer-policy': 'no-referrer',
  'content-security-policy':
    "default-src 'self'; connect-src 'self' ws: wss:; img-src 'self' data: blob:; worker-src 'self' blob:; style-src 'self' 'unsafe-inline'; frame-ancestors 'none'",
}

const CONTENT_TYPES: Record<string, string> = {
  '.css': 'text/css; charset=utf-8',
  '.html': 'text/html; charset=utf-8',
  '.ico': 'image/x-icon',
  '.js': 'text/javascript; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.map': 'application/json; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.png': 'image/png',
  '.svg': 'image/svg+xml',
  '.ttf': 'font/ttf',
  '.txt': 'text/plain; charset=utf-8',
  '.wasm': 'application/wasm',
  '.webp': 'image/webp',
  '.woff': 'font/woff',
  '.woff2': 'font/woff2',
}

// The dashboard asks the daemon through /api, as through Vite and nginx. When the daemon serves the
// dashboard itself, it takes the same addresses.
export function stripApiPrefix(url: string): string {
  if (url === '/api' || url.startsWith('/api?')) {
    return `/${url.slice(4)}`
  }
  return url.startsWith('/api/') ? url.slice(4) : url
}

// A browser that opens an address of the dashboard (a deep link like /w/T-0018/changes) asks for
// HTML. Only that kind of request gets the page: a script that asks for JSON still gets a 404.
export function wantsPage(request: FastifyRequest): boolean {
  return request.method === 'GET' && (request.headers.accept ?? '').includes('text/html')
}

// The file of the built dashboard at an address, or null. Nothing outside the folder is served.
export function webFile(webDir: string, url: string): string | null {
  let path: string
  try {
    path = decodeURIComponent(new URL(url, 'http://localhost').pathname)
  } catch {
    return null
  }
  const root = resolve(webDir)
  const file = resolve(join(root, path))
  const inside = relative(root, file)
  if (inside === '' || inside.startsWith('..') || isAbsolute(inside)) {
    return null
  }
  try {
    return statSync(file).isFile() ? file : null
  } catch {
    return null
  }
}

// Answers a GET that matches no route:
// - with a file of the built dashboard, when the daemon serves the dashboard (webDir);
// - with the dashboard's index.html when it accepts HTML, so the dashboard can read the address
//   and show the right view.
// These hold no data, so they need no token. Returns the check that the token hook uses to let
// such a request through.
export function registerWebFallback(
  app: FastifyInstance,
  indexPath: string,
  webDir?: string,
): (request: FastifyRequest) => boolean {
  const fileFor = (request: FastifyRequest) =>
    webDir !== undefined && request.method === 'GET' ? webFile(webDir, request.url) : null

  app.setNotFoundHandler(async (request, reply) => {
    const file = fileFor(request)
    if (file !== null) {
      try {
        const body = await readFile(file)
        const type = CONTENT_TYPES[extname(file).toLowerCase()] ?? 'application/octet-stream'
        // Vite puts a hash of the content in the names under assets/, so they never change.
        const cache = request.url.startsWith('/assets/')
          ? 'public, max-age=31536000, immutable'
          : 'no-cache'
        reply.type(type).header('cache-control', cache)
        if (type.startsWith('text/html')) {
          reply.headers(PAGE_HEADERS)
        }
        return reply.send(body)
      } catch {
        // Gone since it was found: answer as below.
      }
    }
    if (wantsPage(request)) {
      try {
        const page = await readFile(indexPath, 'utf8')
        return reply
          .type('text/html; charset=utf-8')
          .header('cache-control', 'no-cache')
          .headers(webDir === undefined ? {} : PAGE_HEADERS)
          .send(page)
      } catch {
        // The page is gone: answer as for any other unknown address.
      }
    }
    // The same body that Fastify sends when no handler is set.
    return reply.code(404).send({
      message: `Route ${request.method}:${request.url} not found`,
      error: 'Not Found',
      statusCode: 404,
    })
  })
  return (request) =>
    request.is404 && ((wantsPage(request) && existsSync(indexPath)) || fileFor(request) !== null)
}
