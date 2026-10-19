const LOOPBACK_HOSTS = ['127.0.0.1', 'localhost', '::1']

function hostsOf(list: string | undefined): string[] {
  return (list ?? '')
    .split(',')
    .map((host) => host.trim())
    .filter((host) => host !== '')
}

// Claude Code sends its `http` hooks through HTTP_PROXY, and a company proxy refuses a request to
// loopback, so every hook fails with "proxy refused the connection". Tools read either the upper
// or the lower case name, so both get the same list.
export function withLoopbackNoProxy(env: NodeJS.ProcessEnv): NodeJS.ProcessEnv {
  const hosts: string[] = []
  const seen = new Set<string>()
  for (const host of [...hostsOf(env.NO_PROXY), ...hostsOf(env.no_proxy), ...LOOPBACK_HOSTS]) {
    const key = host.toLowerCase()
    if (!seen.has(key)) {
      seen.add(key)
      hosts.push(host)
    }
  }
  const list = hosts.join(',')
  return { ...env, NO_PROXY: list, no_proxy: list }
}
