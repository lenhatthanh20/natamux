export class ApiError extends Error {
  readonly status: number

  constructor(status: number, message: string) {
    super(message)
    this.name = 'ApiError'
    this.status = status
  }
}

export function isUnauthorized(error: unknown): boolean {
  return error instanceof ApiError && error.status === 401
}

// The daemon refused a save because the file changed since it was read.
export function isConflict(error: unknown): boolean {
  return error instanceof ApiError && error.status === 409
}

export function parseJson(text: string): unknown {
  try {
    return JSON.parse(text)
  } catch {
    return undefined
  }
}

export function describeError(status: number, body: unknown): string {
  if (status === 401) {
    return 'The daemon rejected the token.'
  }
  if (typeof body === 'object' && body !== null && 'message' in body) {
    return String(body.message)
  }
  return `The daemon answered with status ${status}.`
}
