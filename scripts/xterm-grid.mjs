// Replays the snapshot dumps of the contract suite in xterm.js, as the web dashboard does, and
// compares what the Node daemon's snapshot and the Rust daemon's draw: every row of text, the
// colours and attributes of each cell, the cursor, the buffer and the modes
// (docs/rust-phase4.md, "Contract suite"). CI only: it needs the packages of @natamux/daemon.
//
// Trailing spaces of a row do not count: a space a program wrote and an empty cell look the same,
// and the daemon's screen does not keep them apart (docs/rust-phase4.md).
//
// Usage: node scripts/xterm-grid.mjs <dump folder>
// A dump is <scenario>.<daemon>.<cols>x<rows>.bin; daemon is node or rust.

import { readdirSync, readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { join } from 'node:path'

const require = createRequire(new URL('../packages/daemon/package.json', import.meta.url))
const { Terminal } = require('@xterm/headless')
const { Unicode11Addon } = require('@xterm/addon-unicode11')

function replay(bytes, cols, rows) {
  return new Promise((resolve) => {
    // Scrollback as the dashboard's (scrollbackLines in @natamux/core).
    const fit = Math.floor(10_000_000 / (cols * 16))
    const terminal = new Terminal({
      cols,
      rows,
      scrollback: Math.min(10_000, Math.max(1000, fit)),
      allowProposedApi: true,
    })
    terminal.loadAddon(new Unicode11Addon())
    terminal.unicode.activeVersion = '11'
    terminal.write(bytes, () => {
      resolve(grid(terminal))
      terminal.dispose()
    })
  })
}

function cellStyle(cell) {
  return [
    cell.getFgColorMode(),
    cell.getFgColor(),
    cell.getBgColorMode(),
    cell.getBgColor(),
    cell.isBold(),
    cell.isDim(),
    cell.isItalic(),
    cell.isUnderline(),
    cell.isInverse(),
    cell.isInvisible(),
    cell.isStrikethrough(),
  ].join(',')
}

function grid(terminal) {
  const lines = []
  for (const buffer of [terminal.buffer.normal, terminal.buffer.alternate]) {
    for (let row = 0; row < buffer.length; row += 1) {
      const line = buffer.getLine(row)
      if (line === undefined) {
        continue
      }
      const styles = []
      for (let column = 0; column < terminal.cols; column += 1) {
        const cell = line.getCell(column)
        const blank = cell !== undefined && cell.getChars().trim() === '' && cell.isBgDefault()
        if (cell !== undefined && !blank) {
          styles.push(`${column}:${cellStyle(cell)}`)
        }
      }
      lines.push(`${buffer.type} ${row} ${line.isWrapped ? 'w' : '-'} ${line.translateToString(true).trimEnd()} | ${styles.join(' ')}`)
    }
  }
  const active = terminal.buffer.active
  return {
    lines: trimEnd(lines),
    cursor: [active.type, active.cursorX, active.cursorY],
    modes: JSON.stringify(terminal.modes),
  }
}

// Rows that hold nothing at the end of each buffer do not count.
function trimEnd(lines) {
  const out = [...lines]
  while (out.length > 0 && / \d+ - {2}\| $/.test(out.at(-1)) === true) {
    out.pop()
  }
  return out
}

const folder = process.argv[2]
const dumps = new Map()
for (const name of readdirSync(folder)) {
  const match = /^(.+)\.(node|rust)\.(\d+)x(\d+)\.bin$/.exec(name)
  if (match === null) {
    continue
  }
  const [, scenario, daemon, cols, rows] = match
  const entry = dumps.get(scenario) ?? {}
  entry[daemon] = { bytes: readFileSync(join(folder, name)), cols: Number(cols), rows: Number(rows) }
  dumps.set(scenario, entry)
}

let failed = 0
for (const [scenario, { node, rust }] of dumps) {
  if (node === undefined || rust === undefined) {
    console.log(`${scenario}: a dump is missing`)
    failed += 1
    continue
  }
  const [left, right] = await Promise.all([
    replay(node.bytes, node.cols, node.rows),
    replay(rust.bytes, rust.cols, rust.rows),
  ])
  const differences = []
  for (const part of ['cursor', 'modes']) {
    if (JSON.stringify(left[part]) !== JSON.stringify(right[part])) {
      differences.push(`${part}: node ${JSON.stringify(left[part])}, rust ${JSON.stringify(right[part])}`)
    }
  }
  const length = Math.max(left.lines.length, right.lines.length)
  for (let index = 0; index < length && differences.length < 20; index += 1) {
    if (left.lines[index] !== right.lines[index]) {
      differences.push(`row ${index}:\n  node ${left.lines[index]}\n  rust ${right.lines[index]}`)
    }
  }
  if (differences.length === 0) {
    console.log(`${scenario}: the same screen in xterm.js (${left.lines.length} rows)`)
  } else {
    failed += 1
    console.log(`${scenario}: DIFFERS\n${differences.join('\n')}`)
  }
}
process.exit(failed === 0 ? 0 : 1)
