// The landing page: theme, download for this system, tabs, the stage flow and reveals.
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

// The stage flow. Each frame sets the state of every stage; the CSS draws it.
// States: idle, run, wait (for you), done, back (sent the task back).
const box = document.querySelector('[data-flow]')
if (box) {
  const S = (spec) => Object.fromEntries(spec.split(' ').map((p) => p.split(':')))
  const approved = 'Approved by you'
  const frames = [
    { s: S('context:run'), lit: [], step: 0 },
    { s: S('context:done spec:run'), lit: ['e1'], step: 0 },
    { s: S('context:done spec:wait'), lit: ['e1'], step: 0 },
    { s: S('context:done spec:done plan:run'), lit: ['e1', 'e2'], step: 0 },
    { s: S('context:done spec:done plan:wait'), lit: ['e1', 'e2'], step: 0 },
    { s: S('context:done spec:done plan:done impl:run test:run'), lit: ['e1', 'e2', 'e3', 'e4'], step: 1 },
    { s: S('context:done spec:done plan:done impl:run test:wait'), lit: ['e1', 'e2', 'e3', 'e4'], step: 1 },
    { s: S('context:done spec:done plan:done impl:done test:done'), lit: ['e1', 'e2', 'e3', 'e4'], step: 1 },
    { s: S('context:done spec:done plan:done impl:done test:done tests:run'), lit: ['e1', 'e2', 'e3', 'e4', 'e5', 'e6'], step: 2 },
    { s: S('context:done spec:done plan:done impl:done test:done tests:done review:run'), lit: ['e1', 'e2', 'e3', 'e4', 'e5', 'e6', 'e7'], step: 3 },
    { s: S('context:done spec:done plan:done impl:run test:done tests:idle review:back'), lit: ['e1', 'e2', 'e3', 'e4', 'loop'], step: 4, notes: { impl: 'Sent back 1×' } },
    { s: S('context:done spec:done plan:done impl:done test:done tests:run review:idle'), lit: ['e1', 'e2', 'e3', 'e4', 'e5', 'e6'], step: 4, notes: { impl: 'Fixed' } },
    { s: S('context:done spec:done plan:done impl:done test:done tests:done review:run'), lit: ['e1', 'e2', 'e3', 'e4', 'e5', 'e6', 'e7'], step: 4 },
    { s: S('context:done spec:done plan:done impl:done test:done tests:done review:done release:run'), lit: ['e1', 'e2', 'e3', 'e4', 'e5', 'e6', 'e7', 'e8'], step: 5, notes: { review: 'No findings' } },
    { s: S('context:done spec:done plan:done impl:done test:done tests:done review:done release:done'), lit: ['e1', 'e2', 'e3', 'e4', 'e5', 'e6', 'e7', 'e8'], step: 5, notes: { review: 'No findings', release: 'PR opened' } },
  ]
  const doneNotes = { spec: approved, plan: approved, test: approved, tests: 'All pass', impl: 'Done', context: 'Done', review: 'Done', release: 'Merged' }
  const text = { run: 'Running', wait: 'Waits for you', back: '2 findings', idle: ' ' }

  const nodes = [...box.querySelectorAll('[data-n]')]
  const lits = [...box.querySelectorAll('.lit')]
  const items = [...box.querySelectorAll('[data-step]')]
  const play = box.querySelector('[data-flow-play]')
  const playLabel = box.querySelector('[data-flow-play-label]')
  const firstFrame = items.map((_, i) => frames.findIndex((f) => f.step === i))

  let at = 0
  let timer = 0
  let visible = false
  let paused = reduced

  function show(i) {
    at = i
    const f = frames[i]
    for (const node of nodes) {
      const id = node.dataset.n
      const state = f.s[id] || 'idle'
      node.dataset.s = state
      node.querySelector('[data-note]').textContent = f.notes?.[id] || (state === 'done' ? doneNotes[id] : text[state])
    }
    for (const path of lits) path.classList.toggle('on', f.lit.includes(path.dataset.e))
    box.dataset.loop = f.lit.includes('loop') ? 'on' : 'off'
    items.forEach((li, j) => (j === f.step ? li.setAttribute('aria-current', 'step') : li.removeAttribute('aria-current')))
  }

  function tick() {
    clearTimeout(timer)
    if (paused || !visible || document.hidden) return
    const last = at === frames.length - 1
    timer = setTimeout(() => {
      show(last ? 0 : at + 1)
      tick()
    }, last ? 4200 : 1500)
  }

  function setPaused(value) {
    paused = value
    play.setAttribute('aria-pressed', String(paused))
    playLabel.textContent = paused ? 'Play' : 'Pause'
    tick()
  }

  play.addEventListener('click', () => setPaused(!paused))
  for (const button of box.querySelectorAll('[data-step-go]')) {
    button.addEventListener('click', () => {
      show(firstFrame[Number(button.dataset.stepGo)])
      setPaused(true)
    })
  }
  document.addEventListener('visibilitychange', tick)
  new IntersectionObserver(
    ([entry]) => {
      visible = entry.isIntersecting
      box.classList.toggle('live', visible)
      tick()
    },
    { threshold: 0.35 },
  ).observe(box)

  // Without motion, show the loop itself: the most telling frame.
  show(reduced ? 10 : 0)
  setPaused(reduced)
}

// Reveal pictures once, when they come into view.
const reveals = document.querySelectorAll('.reveal')
if (!reduced && 'IntersectionObserver' in window) {
  root.classList.add('js')
  const io = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (!entry.isIntersecting) continue
        entry.target.classList.add('in')
        io.unobserve(entry.target)
      }
    },
    { rootMargin: '0px 0px -10% 0px' },
  )
  for (const el of reveals) io.observe(el)
}
