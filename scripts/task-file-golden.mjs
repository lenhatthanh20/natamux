// Writes the task file fixtures of natamux-store with the gray-matter and js-yaml of the Node
// daemon, so the Rust store is held to their bytes (docs/rust-phase4.md, part 2). Runs in CI only.
//
//   node scripts/task-file-golden.mjs <folder>
//
// For each <name>.data.json ({ data, content }) it writes <name>.md, as matter.stringify does.
// For each <name>.in.md it writes <name>.read.json: the front matter and the body matter() reads,
// a Date as { "$date": "<ISO>" }.
import { readdirSync, readFileSync, writeFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { join } from 'node:path'

const require = createRequire(new URL('../packages/daemon/package.json', import.meta.url))
const matter = require('gray-matter')

const folder = process.argv[2]
if (!folder) {
  console.error('usage: node scripts/task-file-golden.mjs <folder>')
  process.exit(2)
}

function withDates(value) {
  if (value instanceof Date) {
    return { $date: value.toISOString() }
  }
  if (Array.isArray(value)) {
    return value.map(withDates)
  }
  if (value !== null && typeof value === 'object') {
    return Object.fromEntries(Object.entries(value).map(([key, each]) => [key, withDates(each)]))
  }
  return value
}

for (const name of readdirSync(folder).sort()) {
  const path = join(folder, name)
  if (name.endsWith('.data.json')) {
    const { data, content } = JSON.parse(readFileSync(path, 'utf8'))
    writeFileSync(path.replace(/\.data\.json$/, '.md'), matter.stringify(content, data))
  } else if (name.endsWith('.in.md')) {
    let read
    try {
      const file = matter(readFileSync(path, 'utf8'))
      read = { data: withDates(file.data), content: file.content }
    } catch (error) {
      read = { error: true }
    }
    writeFileSync(path.replace(/\.in\.md$/, '.read.json'), `${JSON.stringify(read, null, 2)}\n`)
  }
}
