import { existsSync } from 'node:fs'
import { readFile } from 'node:fs/promises'
import type { FastifyInstance, FastifyRequest } from 'fastify'

// A browser that opens an address of the dashboard (a deep link like /w/T-0018/changes) asks for
// HTML. Only that kind of request gets the page: a script that asks for JSON still gets a 404.
export function wantsPage(request: FastifyRequest): boolean {
  return request.method === 'GET' && (request.headers.accept ?? '').includes('text/html')
}

// Answers a GET that matches no route and accepts HTML with the dashboard's index.html, so the
// dashboard can read the address and show the right view. The page holds no data, so it needs no
// token. Returns the check that the token hook uses to let such a request through.
export function registerWebFallback(
  app: FastifyInstance,
  indexPath: string,
): (request: FastifyRequest) => boolean {
  app.setNotFoundHandler(async (request, reply) => {
    if (wantsPage(request)) {
      try {
        const page = await readFile(indexPath, 'utf8')
        return reply.type('text/html; charset=utf-8').header('cache-control', 'no-cache').send(page)
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
  return (request) => request.is404 && wantsPage(request) && existsSync(indexPath)
}
