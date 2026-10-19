import { type TerminalLane, terminalLaneSchema } from '@natamux/core'
import type { Client } from './client'
import { asRecord, stringList } from './parse'
import type { TaskShells } from './types'

export function terminalsApi(client: Client) {
  const { request } = client

  return {
    // The names of the open terminals of a task, such as ["1", "2"].
    async listTerminals(taskId: string): Promise<string[]> {
      const body = asRecord(await request('GET', `/tasks/${encodeURIComponent(taskId)}/terminals`))
      return stringList(body.terminals)
    },

    // The terminals where an agent runs now, by name, with the stage each one works on.
    async listAgentTerminals(taskId: string): Promise<Record<string, TerminalLane>> {
      const body = asRecord(await request('GET', `/tasks/${encodeURIComponent(taskId)}/terminals`))
      const lanes = body.lanes === undefined ? {} : asRecord(body.lanes)
      return Object.fromEntries(
        stringList(body.agents).map((name) => [
          name,
          terminalLaneSchema.parse(lanes[name] ?? { stage: null, state: null }),
        ]),
      )
    },

    async listShells(taskId: string): Promise<TaskShells> {
      const body = asRecord(await request('GET', `/tasks/${encodeURIComponent(taskId)}/terminals`))
      const lanes = body.lanes === undefined ? {} : asRecord(body.lanes)
      const programs = body.programs === undefined ? {} : asRecord(body.programs)
      const restoring = body.restoring === undefined ? {} : asRecord(body.restoring)
      const terminals = stringList(body.terminals)
      return {
        terminals,
        lanes: Object.fromEntries(
          stringList(body.agents).map((name) => [
            name,
            terminalLaneSchema.parse(lanes[name] ?? { stage: null, state: null }),
          ]),
        ),
        programs: Object.fromEntries(
          terminals.map((name) => {
            const program = programs[name]
            return [name, typeof program === 'string' ? program : null]
          }),
        ),
        restoring: Object.fromEntries(
          Object.entries(restoring)
            .filter(([name]) => terminals.includes(name))
            .map(([name, agent]) => [name, typeof agent === 'string' ? agent : null]),
        ),
      }
    },

    async closeTerminal(taskId: string, terminal: string): Promise<string[]> {
      const body = asRecord(
        await request(
          'DELETE',
          `/tasks/${encodeURIComponent(taskId)}/terminals/${encodeURIComponent(terminal)}`,
        ),
      )
      return stringList(body.terminals)
    },

    // A one-time ticket that opens the terminal socket of a task, see terminalSocketUrl.
    async terminalTicket(taskId: string): Promise<string> {
      const body = asRecord(
        await request('POST', `/tasks/${encodeURIComponent(taskId)}/terminal/ticket`),
      )
      if (typeof body.ticket !== 'string') {
        throw new Error('The daemon sent an answer this dashboard does not understand.')
      }
      return body.ticket
    },
  }
}
