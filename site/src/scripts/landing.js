// Motion for the landing page. Every demo is a loop of small steps that runs only while it is on screen.
const reduce = matchMedia('(prefers-reduced-motion: reduce)').matches
const $ = (sel, root = document) => root.querySelector(sel)
const $$ = (sel, root = document) => [...root.querySelectorAll(sel)]

// Wait until the element is on screen and the page is visible.
const seen = new Map()
const watch = new IntersectionObserver((entries) => {
  for (const entry of entries) {
    const state = seen.get(entry.target)
    state.on = entry.isIntersecting
    if (state.on) wakeAll(state)
  }
}, { rootMargin: '80px' })
function track(el) {
  if (!seen.has(el)) {
    const state = { on: false, waiting: new Set() }
    seen.set(el, state)
    watch.observe(el)
  }
  return seen.get(el)
}
document.addEventListener('visibilitychange', () => {
  if (!document.hidden) for (const state of seen.values()) wakeAll(state)
})
function wakeAll(state) {
  const waiting = [...state.waiting]
  state.waiting.clear()
  for (const check of waiting) check()
}
function onScreen(el) {
  const state = track(el)
  const ready = () => state.on && !document.hidden && !el.closest('[hidden]')
  if (ready()) return Promise.resolve()
  return new Promise((resolve) => {
    const check = () => (ready() ? resolve() : state.waiting.add(check))
    state.waiting.add(check)
  })
}
const sleep = (ms) => new Promise((done) => setTimeout(done, reduce ? 0 : ms))
async function step(el, ms) {
  await onScreen(el)
  await sleep(ms)
  await onScreen(el)
}

// A line in a fake terminal: an array of [text, class] parts, typed out.
function addLine(body, parts, cls = '', keep = 40) {
  const line = document.createElement('div')
  line.className = `l ${cls}`
  const spans = parts.map(([, c]) => {
    const span = document.createElement('span')
    if (c) span.className = c
    line.append(span)
    return span
  })
  body.append(line)
  while (body.children.length > keep) body.firstElementChild.remove()
  return { line, spans, parts }
}
async function typeLine(el, body, parts, cls, speed = 12, keep) {
  const made = addLine(body, typeof parts === 'string' ? [[parts]] : parts, cls, keep)
  for (let p = 0; p < made.parts.length; p++) {
    const text = made.parts[p][0]
    if (reduce || speed === 0) { made.spans[p].textContent = text; continue }
    for (let i = 1; i <= text.length; i += 2) {
      made.spans[p].textContent = text.slice(0, i)
      await sleep(speed)
    }
    made.spans[p].textContent = text
  }
  await onScreen(el)
  return made.line
}

// Move the fake pointer to the middle of a target, inside an .app that may be scaled.
async function pointAt(app, pointer, target, click = true) {
  const box = app.getBoundingClientRect()
  const scale = box.width / app.offsetWidth || 1
  const t = target.getBoundingClientRect()
  const x = (t.left + t.width / 2 - box.left) / scale
  const y = (t.top + t.height / 2 - box.top) / scale
  pointer.classList.add('show')
  pointer.style.transform = `translate(${x}px, ${y}px)`
  await sleep(950)
  if (!click) return
  pointer.classList.remove('click')
  void pointer.getBoundingClientRect()
  pointer.classList.add('click')
  target.classList.add('press')
  setTimeout(() => target.classList.remove('press'), 400)
  await sleep(300)
}
function hidePointer(pointer) {
  pointer.classList.remove('show')
}

// Scale each app replica to the width of its frame.
const fits = new ResizeObserver((entries) => {
  for (const entry of entries) {
    const app = entry.target.firstElementChild
    entry.target.style.setProperty('--scale', String(entry.contentRect.width / size(app).width))
  }
})
// Read the CSS size: a replica in a hidden tab has no layout yet.
function size(app) {
  const style = getComputedStyle(app)
  return { width: parseFloat(style.width), height: parseFloat(style.height) }
}
for (const fit of $$('[data-fit]')) {
  const { width, height } = size(fit.firstElementChild)
  fit.style.setProperty('--ratio', `${width} / ${height}`)
  fits.observe(fit)
}

// Sticky nav gets a line once the page scrolls.
const nav = $('[data-nav]')
const stick = () => nav?.classList.toggle('stuck', scrollY > 8)
addEventListener('scroll', stick, { passive: true })
stick()

// One entrance per block.
const reveals = new IntersectionObserver((entries) => {
  for (const entry of entries) {
    if (entry.isIntersecting) {
      entry.target.classList.add('in')
      reveals.unobserve(entry.target)
    }
  }
}, { threshold: 0.12 })
for (const el of $$('.reveal:not(.in)')) reveals.observe(el)

for (const copy of $$('[data-copy]')) {
  copy.addEventListener('click', async () => {
    const text = copy.previousElementSibling?.textContent ?? ''
    try {
      await navigator.clipboard.writeText(text)
      copy.textContent = 'Copied'
    } catch {
      copy.textContent = 'Select it'
    }
    setTimeout(() => (copy.textContent = 'Copy'), 1600)
  })
}

// ---------- Hero: one task goes from Implement to a pull request ----------
function needsRow(glyph, title, why, ago = 'now') {
  const row = document.createElement('div')
  row.className = 'needs-row'
  row.innerHTML = `<i class="g" data-s="${glyph}"></i><span class="t"></span><span class="ago">${ago}</span><span class="why"></span>`
  row.querySelector('.t').textContent = title
  row.querySelector('.why').textContent = why
  return row
}

async function hero() {
  const app = $('[data-hero]')
  if (!app) return
  const q = (name) => $(`[data-h-${name}]`, app)
  const term = q('term')
  const st = (name) => $(`[data-st="${name}"]`, app)
  const setStage = (name, cls, when = '') => {
    const li = st(name)
    li.classList.remove('run', 'wait', 'done')
    if (cls) li.classList.add(cls)
    li.querySelector('.when').textContent = when
  }
  const setNeeds = (n) => {
    q('pill').textContent = `${n} ${n === 1 ? 'needs' : 'need'} you`
    q('pill').classList.remove('bump')
    void q('pill').offsetWidth
    q('pill').classList.add('bump')
    q('count').textContent = String(n)
    q('need').textContent = `${n} need you`
  }
  const setTask = (state, label, activity) => {
    q('badge').dataset.s = state
    q('badge').textContent = label
    q('glyph').dataset.s = state
    q('tglyph').dataset.s = state
    q('tstate').textContent = activity
  }
  const act = (text) => {
    q('act').textContent = text
    q('tstate').textContent = `Running · ${text}`
  }
  const T = (parts, cls, speed) => typeLine(app, term, parts, cls, speed, 22)
  const tool = (dot, name, arg) => [[`${dot} `, dot === '●' ? 'dot' : 'dot b'], [name, 'tool'], [arg ? ` ${arg}` : '']]

  const ask = q('ask')
  const askGlyph = $('.g', ask)
  const askAct = $('.act', ask)

  for (;;) {
    // Reset.
    term.replaceChildren()
    for (const [name, cls] of [['test', 'run'], ['impl', 'run'], ['review', ''], ['testing', ''], ['release', '']]) setStage(name, cls)
    setTask('running', 'Running', 'Running · Read src/components/CommandPalette.tsx')
    q('done').textContent = '3 of 7'
    q('needs').replaceChildren(needsRow('failed', 'Retry pushes on network errors', 'Testing · 2 tests failed', '4m'))
    q('count').textContent = '1'
    q('pill').textContent = '1 needs you'
    q('need').textContent = '1 need you'
    q('running').textContent = '3 running'
    q('review').classList.remove('show')
    q('pr').textContent = 'Create pull request'
    q('changes').textContent = '3'
    q('toast').classList.remove('show')
    askGlyph.dataset.s = 'running'
    askAct.className = 'act'
    askAct.textContent = 'ReadFile src/auth/refresh.ts'
    hidePointer(q('pointer'))

    await T([['$ ', 'dim'], ['claude --permission-mode auto "Implement the plan in .natamux-task/plan.md"', 'hi']], '', 4)
    await T('')
    await step(app, 300)
    await T(tool('●', 'Read', 'src/components/CommandPalette.tsx'))
    await T([['  ⎿  Read 214 lines', 'dim']])
    await step(app, 500)
    act('Edit src/lib/commands.ts')
    await T(tool('●', 'Update', 'src/lib/commands.ts'))
    await T([['  ⎿  Updated with ', 'dim'], ['42 additions', 'add'], [' and ', 'dim'], ['3 removals', 'del']])
    await T([['     40 + export const commands: Command[] = [', 'add']], '', 6)
    await T([["     41 +   { id: 'task.new', title: 'Start a task', keys: 'Mod+N' },", 'add']], '', 6)
    await step(app, 400)
    setStage('test', 'done', '5m')
    q('done').textContent = '4 of 7'
    act('Bash: pnpm --filter web test')
    await T(tool('●', 'Bash', 'pnpm --filter @natamux/web test'))
    await step(app, 900)
    await T([['  ⎿  ', 'dim'], ['✓ 128 passed', 'ok'], [' (3.1s)', 'dim']])
    await step(app, 400)
    await T(tool('●', 'Bash', 'natamux done'))
    await T([['  ⎿  Implement is done. Checks passed: worktree has changes, tests pass.', 'dim']])
    await step(app, 500)

    // Review waits for you.
    setStage('impl', 'done', '9m')
    setStage('review', 'wait')
    q('done').textContent = '5 of 7'
    setTask('review', 'In review', 'In review · waits for your approval')
    q('review').classList.add('show')
    q('needs').prepend(needsRow('review', 'Command palette with ⌘K', 'Review · waits for approval'))
    q('running').textContent = '2 running'
    setNeeds(2)
    await step(app, 1500)

    // Another agent asks a question.
    askGlyph.dataset.s = 'needs'
    askAct.className = 'act ask'
    askAct.textContent = 'The agent asks: Keep the old ticket format?'
    q('needs').prepend(needsRow('needs', 'Fix the token refresh race', 'Spec · Keep the old ticket format?'))
    q('running').textContent = '1 running'
    setNeeds(3)
    q('needsbox').classList.add('glow')
    await step(app, 1800)
    q('needsbox').classList.remove('glow')
    if (reduce) return

    // You approve.
    await pointAt(app, q('pointer'), q('approve'))
    q('review').classList.remove('show')
    setStage('review', 'done', 'you')
    setStage('testing', 'run')
    q('done').textContent = '6 of 7'
    q('needs').children[1]?.remove()
    setNeeds(2)
    setTask('running', 'Running', 'Running · Testing')
    await step(app, 300)
    hidePointer(q('pointer'))
    await T('')
    await T([['── Testing ──────────────────────────────', 'dim']], '', 3)
    await T([['$ ', 'dim'], ['pnpm test', 'hi']])
    await step(app, 900)
    await T([['  Test Files  ', 'dim'], ['24 passed', 'ok'], [' (24)', 'dim']])
    await T([['       Tests  ', 'dim'], ['131 passed', 'ok'], [' (131)', 'dim']])
    await step(app, 500)
    setStage('testing', 'done', '1m')
    setStage('release', 'run')
    q('done').textContent = '7 of 7'
    await T([['── Release ──────────────────────────────', 'dim']], '', 3)
    await T([['$ ', 'dim'], ['git push -u origin natamux/t-0012-command-palette', 'hi']], '', 6)
    await step(app, 700)
    setStage('release', 'done', 'now')
    q('pr').textContent = 'Open pull request ↗'
    q('changes').textContent = '0'
    setTask('done', 'Done', 'Done · pull request #182')
    q('act').textContent = 'Pull request #182'
    q('toast').classList.add('show')
    await step(app, 3200)
    q('toast').classList.remove('show')
    await step(app, 1200)
  }
}

// ---------- Feature tabs ----------
function showcase() {
  const list = $('[data-show]')
  if (!list) return
  const tabs = $$('[role="tab"]', list)
  let current = 0
  let auto = !reduce
  let timer = 0
  const DUR = 11000
  list.style.setProperty('--dur', `${DUR}ms`)
  const pick = (index, focus = false) => {
    current = (index + tabs.length) % tabs.length
    tabs.forEach((tab, i) => {
      const on = i === current
      tab.setAttribute('aria-selected', String(on))
      tab.tabIndex = on ? 0 : -1
      const panel = document.getElementById(tab.getAttribute('aria-controls'))
      if (panel) panel.hidden = !on
      // Restart the progress bar.
      const bar = $('.bar', tab)
      bar.style.animation = 'none'
      void bar.offsetWidth
      bar.style.animation = ''
    })
    for (const el of $$('[data-demo]')) if (seen.has(el)) wakeAll(seen.get(el))
    if (focus) tabs[current].focus()
    clearTimeout(timer)
    if (auto) timer = setTimeout(next, DUR)
  }
  const next = async () => {
    await onScreen(list)
    pick(current + 1)
  }
  tabs.forEach((tab, i) => {
    tab.addEventListener('click', () => {
      auto = false
      list.classList.add('paused')
      pick(i)
    })
    tab.addEventListener('keydown', (event) => {
      const move = { ArrowRight: 1, ArrowLeft: -1 }[event.key]
      if (!move) return
      auto = false
      list.classList.add('paused')
      pick(current + move, true)
    })
  })
  if (reduce) list.classList.add('paused')
  pick(0)
}

// Needs you: three tasks need you, one after another.
async function needsDemo() {
  const app = $('[data-demo="needs"]')
  if (!app) return
  const list = $('[data-n-list]', app)
  const count = $('[data-n-count]', app)
  const term = $('[data-n-term]', app)
  const notif = $('[data-n-notif]', app)
  const badge = $('[data-n-badge]', app)
  const events = [
    [0, 'needs', 'Fix the token refresh race', 'Spec · Keep the old ticket format?', 'The agent asks: Keep the old ticket format?'],
    [2, 'failed', 'Retry pushes on network errors', 'Testing · 2 tests failed', 'The checks failed: 2 tests'],
    [1, 'review', 'CSV export for the task table', 'Review · waits for approval', 'In review'],
  ]
  for (;;) {
    list.innerHTML = '<div class="needs-empty">Nothing needs you</div>'
    count.textContent = '0'
    term.replaceChildren()
    notif.classList.remove('show')
    badge.dataset.s = 'running'
    badge.textContent = 'Running'
    for (const [i, text] of ['ReadFile src/auth/refresh.ts', 'Edit src/export/csv.ts', 'Bash: pnpm test'].entries()) {
      $(`[data-n-g="${i}"]`, app).dataset.s = 'running'
      const a = $(`[data-n-a="${i}"]`, app)
      a.className = 'act'
      a.textContent = text
    }
    const T = (p, c) => typeLine(app, term, p, c, 12, 16)
    await T([['✦ ', 'dot b'], ['ReadFile', 'tool'], [' src/auth/refresh.ts']])
    await step(app, 500)
    await T([['✦ ', 'dot b'], ['SearchText', 'tool'], [' "ticket" in src/auth']])
    await T([['  Two requests can refresh the token at once.', 'dim']])
    await step(app, 600)
    await T([['✦ ', 'dot b'], ['Shell', 'tool'], [' natamux ask "Keep the old ticket format for one release?"']])
    for (const [n, [i, state, title, why, actText]] of events.entries()) {
      if (n > 0) await step(app, 1500)
      if (list.querySelector('.needs-empty')) list.replaceChildren()
      list.append(needsRow(state, title, why))
      count.textContent = String(n + 1)
      $(`[data-n-g="${i}"]`, app).dataset.s = state
      const a = $(`[data-n-a="${i}"]`, app)
      a.className = state === 'review' ? 'act' : 'act ask'
      a.textContent = actText
      if (n === 0) {
        badge.dataset.s = 'needs'
        badge.textContent = 'Needs input'
        await T([['? Keep the old ticket format for one release? (y/n)', 'ask']])
        await step(app, 400)
        notif.classList.add('show')
      }
    }
    if (reduce) return
    await step(app, 3000)
    notif.classList.remove('show')
    await step(app, 800)
  }
}

// Progress: a board of tasks and stages; markers move.
async function progressDemo() {
  const app = $('[data-demo="progress"]')
  if (!app) return
  const board = $('[data-p-board]', app)
  const names = ['Context', 'Spec', 'Plan', 'Test spec', 'Implement', 'Review', 'Testing', 'Release']
  const head = document.createElement('div')
  head.className = 'pb-head'
  head.innerHTML = '<span>Task</span>' + names.map((n, i) => `<span class="${i === 3 || i === 4 ? 'alt' : ''}">${n}</span>`).join('')
  const tasks = [
    ['Fix the token refresh race', 'T-0004', 1, 'needs'],
    ['Command palette with ⌘K', 'T-0012', 5, 'wait'],
    ['CSV export for the task table', 'T-0005', 4, 'run'],
    ['Retry pushes on network errors', 'T-0006', 6, 'fail'],
    ['Drag tasks between columns', 'T-0013', 2, 'run'],
    ['Keyboard help overlay', 'T-0016', 0, 'run'],
    ['Dark mode for the diff view', 'T-0015', 7, 'run'],
  ]
  const label = { run: ['running', 'Running'], wait: ['review', 'In review'], needs: ['needs', 'Needs input'], fail: ['failed', 'Failed'], done: ['done', 'Done'] }
  const rows = tasks.map(([title, id]) => {
    const row = document.createElement('div')
    row.className = 'pb-row'
    row.innerHTML = `<div class="who"><b></b><span>${id} <span class="badge"></span></span></div>` + names.map((_, i) => `<div class="pb-cell${i === 3 || i === 4 ? ' alt' : ''}"><i></i></div>`).join('')
    $('b', row).textContent = title
    return row
  })
  board.replaceChildren(head, ...rows)
  const paint = (r, at, state) => {
    const cells = $$('.pb-cell', rows[r])
    cells.forEach((cell, i) => {
      cell.className = `pb-cell${i === 3 || i === 4 ? ' alt' : ''}`
      if (i < at || state === 'done') cell.classList.add('done')
      else if (i === at) cell.classList.add(state)
    })
    // Test spec and Implement run side by side.
    if (at === 4 && state === 'run') cells[3].classList.replace('done', 'run')
    const badge = $('.badge', rows[r])
    badge.dataset.s = label[state][0]
    badge.textContent = label[state][1]
  }
  // Each event: [row, stage, state].
  const script = [
    [5, 1, 'run'], [2, 5, 'wait'], [4, 3, 'run'], [0, 2, 'run'], [6, 7, 'done'], [1, 6, 'run'],
    [5, 2, 'run'], [3, 6, 'run'], [4, 4, 'run'], [1, 7, 'run'], [0, 3, 'run'], [3, 7, 'run'],
  ]
  for (;;) {
    tasks.forEach(([, , at, state], r) => paint(r, at, state))
    if (reduce) return
    for (const [r, at, state] of script) {
      await step(app, 1100)
      paint(r, at, state)
    }
    await step(app, 2500)
  }
}

// Project stages: add Security review from the catalog.
async function stagesDemo() {
  const app = $('[data-demo="stages"]')
  if (!app) return
  const list = $('[data-s-list]', app)
  const flow = $('[data-s-flow]', app)
  const catalog = $('[data-s-catalog]', app)
  const pointer = $('[data-s-pointer]', app)
  const base = [
    ['Context', true], ['Spec', true], ['Plan', true], ['group'], ['Review', true], ['Testing', false], ['Release', true],
  ]
  const row = (name, gate, extra = '') => {
    const el = document.createElement('div')
    el.className = `ps-row ${extra}`
    el.innerHTML = `<span class="grip"></span><span class="nm"></span><span class="gate"><i class="${gate ? 'on' : ''}"></i>Wait for my approval</span><span class="switch on"></span>`
    $('.nm', el).textContent = name
    return el
  }
  const draw = (added) => {
    const keep = $$('.ps-title, .ps-tabs', list)
    const rows = []
    for (const [name, gate] of base) {
      if (name === 'group') {
        const g = row('Test spec and Implement', false)
        $('.nm', g).innerHTML = 'Test spec and Implement <small>side by side</small>'
        $('.gate', g).remove()
        rows.push(g, row('Test spec', true, 'sub'), row('Implement', false, 'sub'))
        continue
      }
      rows.push(row(name, gate))
      if (name === 'Review' && added) rows.push(row('Security review', true, 'new'))
    }
    const add = document.createElement('span')
    add.className = 'a-btn ps-add'
    add.textContent = '+ Add stage'
    add.dataset.add = ''
    list.replaceChildren(...keep, ...rows, add)

    const nodes = [document.createElement('h6')]
    nodes[0].textContent = 'Pipeline'
    const node = (name, cls = '') => {
      const el = document.createElement('div')
      el.className = `fc-node ${cls}`
      el.textContent = name
      return el
    }
    const link = () => Object.assign(document.createElement('div'), { className: 'fc-link' })
    for (const [name, gate] of base) {
      if (nodes.length > 1) nodes.push(link())
      if (name === 'group') {
        const pair = document.createElement('div')
        pair.className = 'fc-pair'
        pair.append(node('Test spec', 'gate'), node('Implement'))
        nodes.push(pair)
        continue
      }
      nodes.push(node(name, gate ? 'gate' : ''))
      if (name === 'Review' && added) nodes.push(link(), node('Security review', 'new'))
    }
    flow.replaceChildren(...nodes)
  }
  for (;;) {
    draw(false)
    catalog.classList.remove('show')
    hidePointer(pointer)
    if (reduce) { draw(true); return }
    await step(app, 1200)
    await pointAt(app, pointer, $('[data-add]', list))
    catalog.classList.add('show')
    await step(app, 500)
    const pick = $('[data-s-pick]', catalog)
    pick.classList.add('hi')
    await pointAt(app, pointer, pick)
    pick.classList.remove('hi')
    catalog.classList.remove('show')
    draw(true)
    await step(app, 900)
    hidePointer(pointer)
    await step(app, 3500)
  }
}

// Agents grid: four agents type at once.
async function agentsDemo() {
  const app = $('[data-demo="agents"]')
  if (!app) return
  const grid = $('[data-g-grid]', app)
  const agents = [
    ['T-0012', 'Claude Code', '#e8916b', [
      [['● ', 'dot'], ['Read', 'tool'], [' src/components/CommandPalette.tsx']],
      [['  ⎿  Read 214 lines', 'dim']],
      [['● ', 'dot'], ['Update', 'tool'], [' src/lib/commands.ts']],
      [['  ⎿  ', 'dim'], ['+42', 'add'], [' ', ''], ['−3', 'del']],
      [['● ', 'dot'], ['Bash', 'tool'], [' pnpm test']],
      [['  ⎿  ', 'dim'], ['✓ 128 passed', 'ok']],
    ]],
    ['T-0013', 'Codex', '#e6e7ea', [
      [['› ', 'dim'], ['apply_patch', 'tool'], [' src/components/Board.tsx']],
      [['  ', ''], ['+88', 'add'], [' ', ''], ['−12', 'del']],
      [['› ', 'dim'], ['run', 'tool'], [' pnpm --filter web test']],
      [['  ✓ 61 passed', 'ok']],
      [['› ', 'dim'], ['apply_patch', 'tool'], [' src/lib/drag.ts']],
    ]],
    ['T-0004', 'Gemini CLI', '#7fa8ff', [
      [['✦ ', 'dot b'], ['ReadFile', 'tool'], [' src/auth/refresh.ts']],
      [['✦ Two requests refresh the token at once.', '']],
      [['✦ ', 'dot b'], ['Shell', 'tool'], [' natamux ask']],
      [['? Keep the old ticket format for one release?', 'ask']],
    ]],
    ['T-0015', 'Cursor', '#58d3a6', [
      [['⬢ ', 'dot'], ['Reading', 'tool'], [' src/diff/DiffView.tsx']],
      [['⬢ ', 'dot'], ['Editing', 'tool'], [' src/diff/theme.ts']],
      [['  ', ''], ['+31', 'add'], [' ', ''], ['−8', 'del']],
      [['⬢ ', 'dot'], ['Running', 'tool'], [' pnpm lint']],
      [['  ✓ no problems', 'ok']],
    ]],
  ]
  const panes = agents.map(([id, name, color]) => {
    const term = document.createElement('div')
    term.className = 'term'
    term.innerHTML = `<div class="term-head"><i class="g" data-s="running"></i><span class="mono">${id}</span><span class="agent-dot" style="--c:${color}"></span><span class="agent-name">${name}</span><span class="end">Running</span></div><div class="term-body"></div>`
    return term
  })
  grid.replaceChildren(...panes)
  const lane = async (i, delay) => {
    const [, , , lines] = agents[i]
    const body = $('.term-body', panes[i])
    await sleep(delay)
    for (const parts of lines) {
      await typeLine(app, body, parts, '', 14, 12)
      await step(app, 500 + i * 120)
    }
  }
  for (;;) {
    for (const pane of panes) {
      $('.term-body', pane).replaceChildren()
      pane.classList.remove('flag')
      $('.g', pane).dataset.s = 'running'
      $('.end', pane).textContent = 'Running'
    }
    await Promise.all([0, 1, 2, 3].map((i) => lane(i, i * 350)))
    const gem = panes[2]
    gem.classList.add('flag')
    $('.g', gem).dataset.s = 'needs'
    $('.end', gem).textContent = 'Needs your input'
    $('.g', panes[0]).dataset.s = 'review'
    $('.end', panes[0]).textContent = 'In review'
    if (reduce) return
    await step(app, 4000)
  }
}

// Review: write a note on a line and send the task back.
async function reviewDemo() {
  const app = $('[data-demo="review"]')
  if (!app) return
  const pointer = $('[data-r-pointer]', app)
  const comment = $('[data-r-comment]', app)
  const text = $('[data-r-text]', app)
  const badge = $('[data-r-badge]', app)
  const bar = $('[data-r-bar]', app)
  const note = 'Use the Kbd component for the keys, like the rest of the app.'
  const line = $$('.add', $('[data-r-diff]', app))[1]
  for (;;) {
    comment.classList.remove('show')
    text.textContent = ''
    badge.dataset.s = 'review'
    badge.textContent = 'In review'
    bar.classList.add('show')
    hidePointer(pointer)
    if (reduce) { comment.classList.add('show'); text.textContent = note; return }
    await step(app, 1200)
    await pointAt(app, pointer, line)
    comment.classList.add('show')
    for (let i = 1; i <= note.length; i += 2) {
      text.textContent = note.slice(0, i)
      await sleep(22)
    }
    text.textContent = note
    await step(app, 500)
    await pointAt(app, pointer, $('[data-r-back]', app))
    bar.classList.remove('show')
    badge.dataset.s = 'running'
    badge.textContent = 'Running'
    await step(app, 600)
    hidePointer(pointer)
    await step(app, 3200)
  }
}

// ---------- Stage flow: a task travels; it stops where you approve ----------
async function flowDemo() {
  const svg = $('[data-flow]')
  if (!svg) return
  const lit = $('[data-path]', svg)
  const token = $('[data-token]', svg)
  const token2 = $('[data-token2]', svg)
  // The lower branch, from the fork to the join.
  const lower = $('[data-path2]', svg)
  const len = lit.getTotalLength()
  lit.style.setProperty('--len', String(len))
  const lowerLen = lower.getTotalLength()
  lower.style.setProperty('--len', String(lowerLen))
  const nodes = $$('.node', svg)
  // Where each node starts along the lit path (x of the node's left edge).
  const stops = [
    [10, 0], [150, 1], [290, 2], [470, 3], [680, 4], [820, 5], [960, 6],
  ]
  const lengthAtX = (x) => {
    let lo = 0
    let hi = len
    for (let i = 0; i < 30; i++) {
      const mid = (lo + hi) / 2
      if (lit.getPointAtLength(mid).x < x) lo = mid
      else hi = mid
    }
    return lo
  }
  const marks = stops.map(([x, n]) => [lengthAtX(x + 1), n])
  const forkAt = lengthAtX(391)
  const joinAt = lengthAtX(679)
  const light = (n, cls = 'lit') => nodes.filter((g) => g.dataset.node === String(n)).forEach((g) => g.classList.add(cls))
  const frame = () => new Promise(requestAnimationFrame)

  for (;;) {
    nodes.forEach((g) => g.classList.remove('lit', 'hold'))
    lit.style.strokeDashoffset = String(len)
    lower.style.strokeDashoffset = String(lowerLen)
    token.setAttribute('opacity', '1')
    token2.setAttribute('opacity', '0')
    if (reduce) {
      lit.style.strokeDashoffset = '0'
      lower.style.strokeDashoffset = '0'
      nodes.forEach((g) => g.classList.add('lit'))
      token.setAttribute('opacity', '0')
      return
    }
    await onScreen(svg)
    let at = 0
    let next = 0
    const speed = 0.22 // units per ms
    let last = performance.now()
    while (at < len) {
      await frame()
      await onScreen(svg)
      const now = performance.now()
      const dt = Math.min(now - last, 50)
      last = now
      at = Math.min(len, at + dt * speed)
      lit.style.strokeDashoffset = String(len - at)
      const p = lit.getPointAtLength(at)
      token.setAttribute('cx', String(p.x))
      token.setAttribute('cy', String(p.y))
      const part = Math.min(1, Math.max(0, (at - forkAt) / (joinAt - forkAt)))
      lower.style.strokeDashoffset = String(lowerLen * (1 - part))
      if (at > forkAt && at < joinAt) {
        const q = lower.getPointAtLength(part * lowerLen)
        token2.setAttribute('opacity', '1')
        token2.setAttribute('cx', String(q.x))
        token2.setAttribute('cy', String(q.y))
      } else token2.setAttribute('opacity', '0')
      while (next < marks.length && at >= marks[next][0]) {
        const n = marks[next][1]
        light(n)
        next++
        const gate = nodes.find((g) => g.dataset.node === String(n) && 'gate' in g.dataset)
        if (gate) {
          gate.classList.add('hold')
          await step(svg, 1400)
          gate.classList.remove('hold')
          last = performance.now()
        }
      }
    }
    light(6)
    token.setAttribute('opacity', '0')
    await step(svg, 2600)
  }
}

// ---------- CLI ----------
async function cliDemo() {
  const pre = $('[data-cli]')
  if (!pre) return
  const lines = [
    [['❯ ', 'p'], ['nmx task create "Fix the login redirect" --agent codex', 'c']],
    [['Created ', 'o'], ['T-0021', 'y'], [' Fix the login redirect', 'o']],
    [''],
    [['❯ ', 'p'], ['nmx agent start T-0021', 'c']],
    [['Workspace ', 'o'], ['natamux/t-0021-fix-the-login-redirect', 'p2']],
    [['Started ', 'o'], ['codex', 'v'], [' in T-0021', 'o']],
    [''],
    [['❯ ', 'p'], ['nmx task list --state running', 'c']],
    [['T-0012  ', 'y'], ['Implement  ', 'o'], ['claude-code  ', 'v'], ['Command palette with ⌘K', '']],
    [['T-0013  ', 'y'], ['Plan       ', 'o'], ['codex        ', 'v'], ['Drag tasks between columns', '']],
    [['T-0021  ', 'y'], ['Context    ', 'o'], ['codex        ', 'v'], ['Fix the login redirect', '']],
    [''],
    [['❯ ', 'p'], ['nmx stage approve T-0012', 'c']],
    [['T-0012 ', 'y'], ['Review approved. Testing starts.', 'g2']],
  ]
  for (;;) {
    pre.replaceChildren()
    for (const parts of lines) {
      const typed = parts[0]?.[1] === 'p'
      const line = document.createElement('div')
      pre.append(line)
      for (const [text, cls] of parts) {
        const span = document.createElement('span')
        if (cls) span.className = cls
        line.append(span)
        if (typed && cls === 'c' && !reduce) {
          await step(pre, 500)
          for (let i = 1; i <= text.length; i++) {
            span.textContent = text.slice(0, i)
            await sleep(28)
          }
        } else span.textContent = text || '​'
      }
      await step(pre, typed ? 450 : 120)
    }
    const caret = document.createElement('span')
    caret.innerHTML = '<span class="p">❯ </span><span class="caret"></span>'
    pre.append(caret)
    if (reduce) return
    await step(pre, 6500)
  }
}

// Hooks tile: events light up in turn.
async function hooksDemo() {
  const row = $('[data-hooks]')
  if (!row || reduce) return
  const items = $$('span', row)
  for (let i = 0; ; i = (i + 1) % items.length) {
    items.forEach((el, j) => el.classList.toggle('on', j === i))
    await step(row, 900)
  }
}

hero()
showcase()
needsDemo()
progressDemo()
stagesDemo()
agentsDemo()
reviewDemo()
flowDemo()
cliDemo()
hooksDemo()
