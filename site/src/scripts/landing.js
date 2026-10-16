// The landing page: theme, recordings, tabs, download for this system, film chapters and the
// living workflow of the hero.
const root = document.documentElement
const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches

// Theme. Same storage key as the docs, so the choice carries over.
const toggle = document.querySelector('[data-theme-toggle]')
function labelTheme() {
  const next = root.dataset.theme === 'light' ? 'dark' : 'light'
  toggle?.setAttribute('aria-label', `Switch to ${next} theme`)
}
labelTheme()
toggle?.addEventListener('click', () => {
  root.dataset.theme = root.dataset.theme === 'light' ? 'dark' : 'light'
  try {
    localStorage.setItem('starlight-theme', root.dataset.theme)
  } catch {}
  labelTheme()
})

// Recordings. A player has one video per theme; only the shown one loads and plays. It plays while
// it is half in view, the tab is visible and its tab panel is open. With reduced motion it waits
// for the Play button. Only the shown video gets its poster, once the player comes near the view.
const players = [...document.querySelectorAll('[data-player]')].map((box) => {
  const p = {
    box,
    videos: [...box.querySelectorAll('video')],
    button: box.querySelector('[data-play]'),
    label: box.querySelector('[data-play-label]'),
    wanted: !reduced,
    seen: false,
    near: false,
    shown: null,
  }
  p.button.addEventListener('click', () => {
    p.wanted = !p.wanted
    sync(p)
  })
  return p
})

function sync(p) {
  const v = p.videos.find((video) => video.classList.contains(root.dataset.theme === 'light' ? 'for-light' : 'for-dark'))
  if (p.shown && p.shown !== v) {
    p.shown.pause()
    // Go on from the same moment. A video that has not loaded yet takes the time once it can.
    const t = p.shown.currentTime
    if (v.readyState) v.currentTime = t
    else v.addEventListener('loadedmetadata', () => (v.currentTime = t), { once: true })
  }
  p.shown = v
  if (p.near && !v.getAttribute('poster')) v.poster = v.dataset.poster
  if (p.wanted && p.seen && !document.hidden && !p.box.closest('[inert]')) {
    v.play().catch((error) => {
      // A pause that cuts a play short is fine. Otherwise nothing plays: offer the button.
      if (error.name === 'AbortError') return
      p.wanted = false
      renderPlayer(p)
    })
  } else {
    v.pause()
  }
  renderPlayer(p)
}

function renderPlayer(p) {
  p.button.toggleAttribute('data-paused', !p.wanted)
  p.label.textContent = p.wanted ? 'Pause' : 'Play'
}

const syncAll = () => players.forEach(sync)
const inView = new IntersectionObserver(
  (entries) => {
    for (const entry of entries) {
      const p = players.find((player) => player.box === entry.target)
      p.seen = entry.isIntersecting
      sync(p)
    }
  },
  { threshold: 0.5 },
)
const nearView = new IntersectionObserver(
  (entries) => {
    for (const entry of entries) {
      if (!entry.isIntersecting) continue
      const p = players.find((player) => player.box === entry.target)
      p.near = true
      nearView.unobserve(p.box)
      sync(p)
    }
  },
  { rootMargin: '600px 0px' },
)
for (const p of players) {
  renderPlayer(p)
  inView.observe(p.box)
  nearView.observe(p.box)
}
document.addEventListener('visibilitychange', syncAll)
new MutationObserver(syncAll).observe(root, { attributes: true, attributeFilter: ['data-theme'] })

// Tabs: arrow keys move between tabs, the panel under each tab fades in.
function selectTab(tabs, tab) {
  for (const t of tabs) {
    const on = t === tab
    t.setAttribute('aria-selected', String(on))
    t.tabIndex = on ? 0 : -1
    const panel = document.getElementById(t.getAttribute('aria-controls'))
    panel.dataset.active = String(on)
    panel.inert = !on
  }
  // A recording in a closed panel stops; one in the opened panel may start.
  syncAll()
}
for (const list of document.querySelectorAll('[role="tablist"]')) {
  const tabs = [...list.querySelectorAll('[role="tab"]')]
  const vertical = list.getAttribute('aria-orientation') === 'vertical'
  list.addEventListener('click', (e) => {
    const tab = e.target.closest('[role="tab"]')
    if (tab) selectTab(tabs, tab)
  })
  list.addEventListener('keydown', (e) => {
    const i = tabs.indexOf(document.activeElement)
    if (i < 0) return
    const back = vertical ? ['ArrowUp', 'ArrowLeft'] : ['ArrowLeft']
    const next = vertical ? ['ArrowDown', 'ArrowRight'] : ['ArrowRight']
    let j = -1
    if (back.includes(e.key)) j = (i - 1 + tabs.length) % tabs.length
    else if (next.includes(e.key)) j = (i + 1) % tabs.length
    else if (e.key === 'Home') j = 0
    else if (e.key === 'End') j = tabs.length - 1
    if (j < 0) return
    e.preventDefault()
    tabs[j].focus()
    selectTab(tabs, tabs[j])
  })
}

// Download: pick the file for this system. Safari and Firefox do not tell
// the Mac's chip, so a Mac gets Apple silicon with Intel right beside it.
async function detect() {
  const ua = navigator.userAgent
  const data = navigator.userAgentData
  const platform = data?.platform || navigator.platform || ''
  if (/iPhone|iPad|iPod|Android/i.test(ua) || (/Mac/.test(platform) && navigator.maxTouchPoints > 1)) return { os: 'phone' }
  if (/Win/i.test(platform) || /Windows/.test(ua)) return { os: 'win' }
  if (/Mac/i.test(platform) || /Mac OS X/.test(ua)) {
    let arch = 'arm64'
    let known = false
    try {
      const v = await data?.getHighEntropyValues(['architecture'])
      if (v?.architecture) known = true
      if (v?.architecture === 'x86') arch = 'x64'
    } catch {}
    return { os: 'mac', arch, known }
  }
  if (/Linux|X11/i.test(platform + ua)) return { os: 'linux' }
  return { os: 'other' }
}

const names = { mac: 'macOS', win: 'Windows', linux: 'Linux' }
detect().then(({ os, arch, known }) => {
  const hero = document.querySelector('[data-hero-download]')
  const label = document.querySelector('[data-hero-label]')
  const note = document.querySelector('[data-hero-note]')
  const detectNote = document.querySelector('[data-dl-detect]')
  const version = note.dataset.version

  if (os === 'phone' || os === 'other') {
    hero.href = '#download'
    label.textContent = 'Get it for your computer'
    note.textContent = `Version ${version} for macOS, Windows and Linux.`
    if (os === 'phone') {
      detectNote.textContent = 'Natamux is a desktop app. Open this page on your computer, or send yourself a link.'
      detectNote.hidden = false
    }
    return
  }

  const panel = document.getElementById(`panel-${os}`)
  let main = panel.querySelector('.btn.primary')
  if (os === 'mac' && arch === 'x64') {
    const [silicon, intel] = panel.querySelectorAll('[data-arch]')
    silicon.classList.remove('primary')
    intel.classList.add('primary')
    intel.after(silicon)
    main = intel
  }
  hero.href = main.href
  label.textContent = `Download for ${names[os]}`
  const what = { mac: arch === 'x64' ? 'Intel Macs' : 'Apple silicon', win: 'Windows 10 and 11', linux: 'Linux (AppImage)' }[os]
  note.textContent = `Version ${version} for ${what}.`
  detectNote.textContent =
    os !== 'mac'
      ? `This looks like ${names[os]}.`
      : known
        ? `This looks like a Mac with ${arch === 'x64' ? 'an Intel chip' : 'Apple silicon'}.`
        : 'This looks like a Mac. If it has an Intel chip, pick Intel.'
  detectNote.hidden = false

  const tabs = [...document.querySelectorAll('.dl-tabs [role="tab"]')]
  selectTab(tabs, document.getElementById(`tab-${os}`))
})

// Copy buttons
for (const button of document.querySelectorAll('[data-copy]')) {
  button.addEventListener('click', async () => {
    const text = button.previousElementSibling.textContent
    try {
      await navigator.clipboard.writeText(text)
      button.textContent = 'Copied'
    } catch {
      button.textContent = 'Select and copy'
    }
    setTimeout(() => (button.textContent = 'Copy'), 1600)
  })
}

// Film chapters: a click jumps there and plays; the chapter that plays fills its bar.
const film = players.find((p) => p.box.id === 'film-player')
const chapters = [...document.querySelectorAll('[data-chapters] li')]
if (film && chapters.length) {
  const mark = () => {
    const t = film.shown?.currentTime ?? 0
    for (const li of chapters) {
      const at = Number(li.dataset.at)
      const end = Number(li.dataset.end)
      li.style.setProperty('--p', String(Math.min(1, Math.max(0, (t - at) / (end - at)))))
      const button = li.querySelector('button')
      if (t >= at && t < end) button.setAttribute('aria-current', 'true')
      else button.removeAttribute('aria-current')
    }
  }
  for (const v of film.videos) v.addEventListener('timeupdate', () => v === film.shown && mark())
  for (const li of chapters) {
    li.querySelector('button').addEventListener('click', () => {
      const at = Number(li.dataset.at)
      film.wanted = true
      film.seen = true
      sync(film)
      const v = film.shown
      if (v.readyState) v.currentTime = at
      else v.addEventListener('loadedmetadata', () => (v.currentTime = at), { once: true })
      mark()
    })
  }
  mark()
}

// The living workflow. One clock drives three tasks through the graph; every frame sets the state
// of each node, wire, label and line from the clock, so pausing or seeking is just not ticking.
const lw = document.querySelector('[data-lw]')
if (lw) livingWorkflow(lw)

function livingWorkflow(box) {
  const LOOP = 24
  // The frame shown without motion: T-0011 waits for you at Manual review.
  const STILL = 19
  const PRESS = 19.8
  // A hold is [from, to, node, state]. A move is [from, to, [wires], state?].
  const tracks = {
    'T-0011': [
      [0, 1, 'spec', 'wait'],
      [1, 1.6, 'spec', 'pass'],
      [1.6, 2.4, ['spec-triage']],
      [2.4, 2.9, 'triage', 'run'],
      [2.9, 3.7, ['triage-reproduce']],
      [3.7, 5, 'reproduce', 'run'],
      [5, 6, ['reproduce-join', 'join-implement']],
      [6, 8, 'implement', 'run'],
      [8, 8.8, ['implement-agent-review']],
      [8.8, 9.8, 'agent-review', 'run'],
      [9.8, 10.2, 'agent-review', 'back'],
      [10.2, 11.4, ['back'], 'back'],
      [11.4, 12.8, 'implement', 'run'],
      [12.8, 13.6, ['implement-agent-review']],
      [13.6, 14.5, 'agent-review', 'run'],
      [14.5, 14.8, 'agent-review', 'pass'],
      [14.8, 15.6, ['agent-review-security']],
      [15.6, 16.8, 'security', 'run'],
      [16.8, 17.6, ['security-review']],
      [17.6, 20, 'review', 'wait'],
      [20, 20.6, 'review', 'pass'],
      [20.6, 21.4, ['review-release']],
      [21.4, 23.4, 'release', 'wait'],
    ],
    // The same task in the other lane of the side by side group.
    'T-0011b': [
      [5, 6, ['reproduce-join', 'join-test-spec']],
      [6, 7.4, 'test-spec', 'run'],
      [7.4, 8, 'test-spec', 'pass'],
      [8, 8.8, ['test-spec-agent-review']],
    ],
    'T-0012': [
      [0, 1.8, 'context', 'pass'],
      [1.8, 2.6, ['context-spec']],
      [2.6, 3.6, 'spec', 'run'],
      [3.6, 4.4, 'spec', 'wait'],
      [4.4, 4.8, 'spec', 'pass'],
      [4.8, 5.6, ['spec-triage']],
      [5.6, 6, 'triage', 'run'],
      [6, 6.8, ['triage-plan']],
      [6.8, 23.4, 'plan', 'wait'],
    ],
    'T-0014': [
      [12, 13.6, 'reproduce', 'run'],
      [13.6, 14.6, ['reproduce-join', 'join-implement']],
      [14.6, 23.4, 'implement', 'run'],
    ],
    'T-0014b': [
      [13.6, 14.6, ['reproduce-join', 'join-test-spec']],
      [14.6, 18, 'test-spec', 'run'],
      [18, 23.4, 'test-spec', 'pass'],
    ],
  }
  // [from, to, wire, label]: the path a task took stays lit for a moment.
  const took = [
    [2.5, 5, 'triage-reproduce', 'bug'],
    [5.7, 8.7, 'triage-plan', 'feature'],
  ]
  const twice = [11.4, 14.4]
  const cursor = { from: 18.2, at: 19.6, to: 20.5, node: 'review' }
  const rank = { pass: 1, run: 2, back: 3, wait: 4 }

  const map = box.querySelector('[data-lw-map]')
  const byData = (key) => Object.fromEntries([...map.querySelectorAll(`[data-${key}]`)].map((el) => [el.getAttribute(`data-${key}`), el]))
  const nodes = byData('n')
  const tokens = byData('t')
  const labels = byData('label')
  const pointer = map.querySelector('.lw-cursor')
  const ticker = [...box.querySelectorAll('.lw-ticker li')]
  const code = box.querySelector('[data-lw-code]')
  const lines = [...code.querySelectorAll('[data-n]')]
  const button = box.querySelector('[data-lw-play]')
  const label = box.querySelector('[data-lw-label]')
  const tall = matchMedia('(max-width: 759px)')

  let clock = reduced ? STILL : 0
  let still = reduced
  let looped = false
  let wanted = !reduced
  let seen = false
  let running = false
  let raf = 0
  let prev = 0
  let hover = null
  let lastFocus = null
  let lit = null
  let wires = {}
  let centre = {}
  let half = {}
  let pad = 0
  let scale = 1

  const put = (el, attr, value) => {
    if (value == null) {
      if (el.hasAttribute(attr)) el.removeAttribute(attr)
    } else if (el.getAttribute(attr) !== value) el.setAttribute(attr, value)
  }
  const ease = (p) => (p < 0.5 ? 2 * p * p : 1 - (2 - 2 * p) ** 2 / 2)

  // Read the geometry of the graph that shows now: wide, or tall on phones.
  function layout() {
    const svg = map.querySelector(tall.matches ? '.lw-wires.tall' : '.lw-wires.wide')
    for (const path of map.querySelectorAll('[data-e]')) path.removeAttribute('data-on')
    wires = {}
    for (const path of svg.querySelectorAll('[data-e]')) {
      wires[path.dataset.e] = { el: path, len: path.getTotalLength(), from: path.dataset.from, to: path.dataset.to }
    }
    for (const [id, node] of Object.entries(nodes)) {
      const s = node.style
      centre[id] = tall.matches
        ? [Number(s.getPropertyValue('--mx')), Number(s.getPropertyValue('--my'))]
        : [Number(s.getPropertyValue('--x')), Number(s.getPropertyValue('--y'))]
      half[id] = node.offsetHeight / 2
    }
    pad = tokens['T-0011'].offsetHeight / 2 + 3
    scale = (tall.matches ? 360 : 1020) / (map.clientWidth || 1)
    draw(clock)
  }

  function place(el, x, y, lift) {
    el.style.setProperty('--x', x.toFixed(1))
    el.style.setProperty('--y', y.toFixed(1))
    if (lift != null) el.style.setProperty('--lift', lift.toFixed(1))
  }

  function draw(t) {
    const state = {}
    const on = {}
    let newest = -1
    let focus = null
    for (const [id, track] of Object.entries(tracks)) {
      const el = tokens[id]
      const seg = track.find(([from, to]) => t >= from && t < to)
      put(el, 'data-on', seg ? '' : null)
      if (!seg) continue
      const [from, to, where, s] = seg
      if (typeof where === 'string') {
        if ((rank[s] || 0) > (rank[state[where]] || 0)) state[where] = s
        if (from > newest) {
          newest = from
          focus = where
        }
        put(el, 'data-s', s)
        place(el, centre[where][0], centre[where][1], half[where] + pad)
      } else {
        const p = (t - from) / (to - from)
        let d = ease(p) * where.reduce((sum, w) => sum + wires[w].len, 0)
        let i = 0
        while (i < where.length - 1 && d > wires[where[i]].len) d -= wires[where[i++]].len
        const point = wires[where[i]].el.getPointAtLength(d)
        // Lift off the node it leaves, ride the wire, settle on top of the next node.
        const a = half[wires[where[0]].from] + pad
        const b = half[wires[where.at(-1)].to] + pad
        put(el, 'data-s', s || 'move')
        place(el, point.x, point.y, (a + (b - a) * p) * (1 - Math.sin(Math.PI * p)))
        for (const w of where) on[w] = s === 'back' ? 'back' : 'go'
      }
    }
    for (const [from, to, wire, key] of took) {
      const now = t >= from && t < to
      if (now) on[wire] = 'took'
      put(labels[key], 'data-on', now ? '' : null)
    }
    for (const [id, node] of Object.entries(nodes)) put(node, 'data-s', state[id] || null)
    for (const [id, wire] of Object.entries(wires)) put(wire.el, 'data-on', on[id] || null)
    put(box, 'data-twice', t >= twice[0] && t < twice[1] ? '' : null)

    // After the first loop, a line that has not come round yet stays, dimmed, from the last loop.
    const count = ticker.filter((li) => t >= Number(li.dataset.at)).length
    ticker.forEach((li, i) => {
      put(li, 'data-on', i < count ? '' : looped ? 'old' : null)
      put(li, 'data-new', i === count - 1 ? '' : null)
    })

    const showCursor = !still && t >= cursor.from && t < cursor.to
    put(pointer, 'data-on', showCursor ? '' : null)
    if (showCursor) {
      const [x, y] = centre[cursor.node]
      // The tip lands on Approve, the lower line of the node.
      const ty = y + half[cursor.node] * 0.4 * scale
      const [sx, sy] = tall.matches ? [x + 120, y + 90] : [x + 140, y + 160]
      const p = ease(Math.min(1, (t - cursor.from) / (cursor.at - cursor.from)))
      place(pointer, sx + (x - sx) * p, sy + (ty - sy) * p)
    }

    // While every task is between stages, keep the lines of the last one.
    if (focus) lastFocus = focus
    showLines(hover || lastFocus)
  }

  function showLines(id) {
    if (id === lit) return
    lit = id
    let first = null
    for (const line of lines) {
      const match = line.dataset.n === id
      put(line, 'data-hl', match ? '' : null)
      if (match && !first) first = line
    }
    if (first && code.scrollHeight > code.clientHeight) {
      code.scrollTo({ top: first.offsetTop - code.clientHeight / 3, behavior: running ? 'smooth' : 'auto' })
    }
  }

  function press() {
    pointer.classList.remove('press')
    void pointer.offsetWidth
    pointer.classList.add('press')
    setTimeout(() => pointer.classList.remove('press'), 600)
  }

  function frame(now) {
    const dt = Math.min((now - prev) / 1000, 0.1)
    prev = now
    const before = clock
    clock = (clock + dt) % LOOP
    if (clock < before) looped = true
    if (before < PRESS && clock >= PRESS) press()
    draw(clock)
    raf = requestAnimationFrame(frame)
  }

  function update() {
    const go = wanted && seen && !document.hidden
    if (go && !running) {
      running = true
      still = false
      prev = performance.now()
      raf = requestAnimationFrame(frame)
    } else if (!go && running) {
      running = false
      cancelAnimationFrame(raf)
    }
    button.toggleAttribute('data-paused', !wanted)
    label.textContent = wanted ? 'Pause' : 'Play'
  }

  button.addEventListener('click', () => {
    wanted = !wanted
    update()
  })
  document.addEventListener('visibilitychange', update)
  new IntersectionObserver(
    ([entry]) => {
      seen = entry.isIntersecting
      update()
    },
    { threshold: 0.15 },
  ).observe(box)

  // Point at or focus a node to see its lines in the file, and the other way round.
  for (const [id, node] of Object.entries(nodes)) {
    const show = () => {
      hover = id
      showLines(id)
    }
    const hide = () => {
      hover = null
      draw(clock)
    }
    node.addEventListener('mouseenter', show)
    node.addEventListener('focus', show)
    node.addEventListener('mouseleave', hide)
    node.addEventListener('blur', hide)
  }
  code.addEventListener('mouseover', (e) => {
    const id = e.target.closest('[data-n]')?.dataset.n
    for (const [key, node] of Object.entries(nodes)) put(node, 'data-hl', key === id ? '' : null)
  })
  code.addEventListener('mouseleave', () => {
    for (const node of Object.values(nodes)) put(node, 'data-hl', null)
  })

  // The nodes are one tab stop; arrow keys move between them.
  const order = Object.values(nodes)
  map.addEventListener('keydown', (e) => {
    const i = order.indexOf(document.activeElement)
    if (i < 0) return
    const j = { ArrowRight: i + 1, ArrowDown: i + 1, ArrowLeft: i - 1, ArrowUp: i - 1, Home: 0, End: order.length - 1 }[e.key]
    if (j === undefined) return
    e.preventDefault()
    const next = order[(j + order.length) % order.length]
    for (const node of order) node.tabIndex = node === next ? 0 : -1
    next.focus()
  })

  new ResizeObserver(layout).observe(map)
  document.fonts?.ready.then(layout)
  layout()
  update()
}
