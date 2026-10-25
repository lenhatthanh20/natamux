import { spawn } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import { median } from './terminal-helpers'

export type RssSample = { at: number; rssMb: number }

const ROOT = fileURLToPath(new URL('../../', import.meta.url))

// Match the daemon executable, not another process that mentions it in an argument.
// The loop waits on stdin instead of sleeping: when the test closes stdin, the loop ends and no
// process is left in the container.
const SCRIPT = `
pid=""
for dir in /proc/[0-9]*; do
  words="$(tr '\\0' '\\n' < "$dir/cmdline" 2>/dev/null)"
  first="$(printf '%s\\n' "$words" | sed -n 1p)"
  case "$first" in
    */natamux-daemon) pid="\${dir#/proc/}"; break ;;
  esac
done
if [ -z "$pid" ]; then
  echo "daemon process not found" >&2
  exit 1
fi
while [ -r "/proc/$pid/status" ]; do
  awk '/VmRSS/ { print $2 }' "/proc/$pid/status"
  read -r -t 0.5 _
  [ $? -gt 128 ] || break
done
`

// The memory of the daemon process only. Its shells and their children are not counted.
export function startRssSampler(): { samples: RssSample[]; stop(): Promise<void> } {
  const samples: RssSample[] = []
  const child = spawn(
    'docker',
    [
      'compose',
      '-p',
      'natamux-e2e',
      '-f',
      'compose.prod.yaml',
      '-f',
      'compose.e2e.yaml',
      'exec',
      '-T',
      'daemon',
      'bash',
      '-c',
      SCRIPT,
    ],
    {
      cwd: ROOT,
      // compose.prod.yaml needs these set, also for exec. They match scripts/e2e.sh.
      env: {
        ...process.env,
        NATAMUX_TOKEN: process.env.NATAMUX_TOKEN ?? 'e2e-token',
        NATAMUX_REPO: 'e2e-repo',
        NATAMUX_PORT: process.env.NATAMUX_PORT ?? '8080',
      },
      stdio: ['pipe', 'pipe', 'pipe'],
    },
  )

  let buffer = ''
  child.stdout.setEncoding('utf8')
  child.stdout.on('data', (chunk: string) => {
    buffer += chunk
    const lines = buffer.split('\n')
    buffer = lines.pop() ?? ''
    for (const line of lines) {
      const kb = Number(line.trim())
      if (line.trim() !== '' && Number.isFinite(kb)) {
        samples.push({ at: Date.now(), rssMb: kb / 1024 })
      }
    }
  })
  child.stderr.setEncoding('utf8')
  child.stderr.on('data', (chunk: string) => console.error(`daemon-rss: ${chunk.trim()}`))
  child.stdin.on('error', () => {})

  const closed = new Promise<void>((resolve) => child.once('close', () => resolve()))

  return {
    samples,
    async stop() {
      child.stdin.end()
      const timer = setTimeout(() => child.kill(), 5_000)
      await closed
      clearTimeout(timer)
    },
  }
}

// before: the median of the last 5 samples before the flood. peak: the highest sample from the
// start of the flood to the end of the window.
export function rssGrowth(
  samples: RssSample[],
  floodStart: number,
  peakEnd: number,
): { beforeMb: number; peakMb: number; growthMb: number } {
  const before = samples.filter((sample) => sample.at < floodStart).slice(-5)
  const during = samples.filter((sample) => sample.at >= floodStart && sample.at <= peakEnd)
  if (before.length === 0 || during.length === 0) {
    throw new Error(`Too few RSS samples: ${before.length} before, ${during.length} during`)
  }
  const beforeMb = median(before.map((sample) => sample.rssMb))
  const peakMb = Math.max(...during.map((sample) => sample.rssMb))
  return { beforeMb, peakMb, growthMb: peakMb - beforeMb }
}
