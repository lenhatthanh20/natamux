import { timingSafeEqual } from 'node:crypto'
import type { FastifyRequest } from 'fastify'

export function isAuthorized(provided: string | undefined, token: string): boolean {
  if (provided === undefined) {
    return false
  }
  const providedBytes = Buffer.from(provided)
  const tokenBytes = Buffer.from(token)
  return providedBytes.length === tokenBytes.length && timingSafeEqual(providedBytes, tokenBytes)
}

export class AgentRefusedError extends Error {
  constructor(what: string) {
    super(`Only a person can ${what}. Use the dashboard.`)
    this.name = 'AgentRefusedError'
  }
}

export function singleHeader(value: string | string[] | undefined): string | undefined {
  return Array.isArray(value) ? value[0] : value
}

export function bearerToken(header: string | undefined): string | undefined {
  return /^Bearer (.+)$/.exec(header ?? '')?.[1]
}

// A caller with the token of the task shells is an agent: it may not do what only a person may.
export function agentChecks(agentToken: string) {
  const byAgent = (request: FastifyRequest) =>
    isAuthorized(bearerToken(request.headers.authorization), agentToken)
  const refuseAgent = (request: FastifyRequest, what: string) => {
    if (byAgent(request)) {
      throw new AgentRefusedError(what)
    }
  }
  return { byAgent, refuseAgent }
}
