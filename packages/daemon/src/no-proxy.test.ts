import { describe, expect, it } from 'vitest'
import { withLoopbackNoProxy } from './no-proxy.js'

describe('withLoopbackNoProxy', () => {
  it('adds the loopback hosts when no list is set', () => {
    const env = withLoopbackNoProxy({ PATH: '/bin' })
    expect(env.NO_PROXY).toBe('127.0.0.1,localhost,::1')
    expect(env.no_proxy).toBe('127.0.0.1,localhost,::1')
  })

  it('keeps the hosts the user set, before the loopback hosts', () => {
    const env = withLoopbackNoProxy({ NO_PROXY: '.corp.example.com' })
    expect(env.NO_PROXY).toBe('.corp.example.com,127.0.0.1,localhost,::1')
    expect(env.no_proxy).toBe(env.NO_PROXY)
  })

  it('uses the lower case list when only that one is set', () => {
    const env = withLoopbackNoProxy({ no_proxy: 'intra.example.com' })
    expect(env.NO_PROXY).toBe('intra.example.com,127.0.0.1,localhost,::1')
    expect(env.no_proxy).toBe(env.NO_PROXY)
  })

  it('joins both lists without duplicates', () => {
    const env = withLoopbackNoProxy({
      NO_PROXY: 'a.example.com,b.example.com',
      no_proxy: 'b.example.com,c.example.com',
    })
    expect(env.NO_PROXY).toBe('a.example.com,b.example.com,c.example.com,127.0.0.1,localhost,::1')
  })

  it('does not add a host that is already there, in any case, and drops empty entries', () => {
    const env = withLoopbackNoProxy({ NO_PROXY: ' LocalHost , ,127.0.0.1,' })
    expect(env.NO_PROXY).toBe('LocalHost,127.0.0.1,::1')
  })

  it('passes other variables through and does not change its input', () => {
    const proxy = 'http://proxy.example.com:3128'
    const input = { PATH: '/bin', HTTP_PROXY: proxy, NO_PROXY: 'x.example.com' }
    const env = withLoopbackNoProxy(input)
    expect(env.PATH).toBe('/bin')
    expect(env.HTTP_PROXY).toBe(proxy)
    expect(input).toEqual({ PATH: '/bin', HTTP_PROXY: proxy, NO_PROXY: 'x.example.com' })
  })
})
