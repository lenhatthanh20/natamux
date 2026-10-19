import { ApiError, describeError, parseJson } from './errors'
import type { FetchFunction } from './types'

export function createClient(token: string, fetchFunction: FetchFunction) {
  async function send(
    method: string,
    path: string,
    body?: unknown,
    headers: Record<string, string> = {},
  ): Promise<Response> {
    try {
      return await fetchFunction(`/api${path}`, {
        method,
        headers: {
          authorization: `Bearer ${token}`,
          ...(body === undefined ? {} : { 'content-type': 'application/json' }),
          ...headers,
        },
        body: body === undefined ? undefined : JSON.stringify(body),
      })
    } catch {
      throw new Error('Cannot reach the daemon. Is it running?')
    }
  }

  async function failure(response: Response): Promise<ApiError> {
    const text = await response.text()
    const parsed = text === '' ? undefined : parseJson(text)
    return new ApiError(response.status, describeError(response.status, parsed))
  }

  async function request(
    method: string,
    path: string,
    body?: unknown,
    headers?: Record<string, string>,
  ): Promise<unknown> {
    const response = await send(method, path, body, headers)
    if (!response.ok) {
      throw await failure(response)
    }
    const text = await response.text()
    return text === '' ? undefined : parseJson(text)
  }

  // For an answer that is not JSON, such as an image.
  async function requestBlob(path: string): Promise<Blob> {
    const response = await send('GET', path)
    if (!response.ok) {
      throw await failure(response)
    }
    return response.blob()
  }

  return { request, requestBlob }
}

export type Client = ReturnType<typeof createClient>
