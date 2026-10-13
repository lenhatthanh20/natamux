import { EventEmitter } from 'node:events'
import { describe, expect, it } from 'vitest'
import { type ParentPort, waitForLoginPath } from './login-path.js'

function fakePort() {
  const emitter = new EventEmitter()
  const port: ParentPort = {
    on: (event, listener) => emitter.on(event, listener),
    postMessage: () => undefined,
  }
  return { port, send: (data: unknown) => emitter.emit('message', { data }) }
}

describe('the login shell PATH from the desktop app', () => {
  it('is waited for, then set for the programs the daemon runs', async () => {
    const env: NodeJS.ProcessEnv = { PATH: '/usr/bin', NATAMUX_PATH_PENDING: '1' }
    const { port, send } = fakePort()
    let path: string | undefined | null = null
    const waiting = waitForLoginPath(env, port).then((answer) => {
      path = answer
    })
    expect('NATAMUX_PATH_PENDING' in env).toBe(false)
    send('ready')
    send({ other: 1 })
    await new Promise((resolve) => setTimeout(resolve, 10))
    expect(path).toBeNull()
    expect(env.PATH).toBe('/usr/bin')

    send({ path: '/opt/homebrew/bin:/usr/bin' })
    await waiting
    expect(path).toBe('/opt/homebrew/bin:/usr/bin')
    expect(env.PATH).toBe('/opt/homebrew/bin:/usr/bin')
  })

  it('keeps the PATH it has when none comes in time, and ignores a late one', async () => {
    const env: NodeJS.ProcessEnv = { PATH: '/usr/bin', NATAMUX_PATH_PENDING: '1' }
    const { port, send } = fakePort()
    expect(await waitForLoginPath(env, port, 5)).toBeUndefined()
    send({ path: '/opt/homebrew/bin:/usr/bin' })
    expect(env.PATH).toBe('/usr/bin')
  })

  it('is not waited for outside the desktop app, or when the app sends none', async () => {
    const env: NodeJS.ProcessEnv = { PATH: '/usr/bin' }
    expect(await waitForLoginPath(env, fakePort().port, 60_000)).toBeUndefined()
    const pending: NodeJS.ProcessEnv = { PATH: '/usr/bin', NATAMUX_PATH_PENDING: '1' }
    expect(await waitForLoginPath(pending, undefined, 60_000)).toBeUndefined()
    expect(pending).toEqual({ PATH: '/usr/bin' })
  })
})
