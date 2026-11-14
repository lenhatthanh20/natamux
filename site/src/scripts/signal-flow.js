function selectTab(tabs, selected) {
  for (const tab of tabs) {
    const active = tab === selected
    tab.setAttribute('aria-selected', String(active))
    tab.tabIndex = active ? 0 : -1
    const panel = document.getElementById(tab.getAttribute('aria-controls'))
    panel.dataset.active = String(active)
    panel.inert = !active
  }
}
for (const list of document.querySelectorAll('[role="tablist"]')) {
  const tabs = [...list.querySelectorAll('[role="tab"]')]
  selectTab(tabs, tabs.find((tab) => tab.getAttribute('aria-selected') === 'true'))
  list.closest('[data-dl]').setAttribute('data-enhanced', '')
  list.addEventListener('click', (event) => {
    const tab = event.target.closest('[role="tab"]')
    if (tab) selectTab(tabs, tab)
  })
  list.addEventListener('keydown', (event) => {
    const index = tabs.indexOf(document.activeElement)
    if (index < 0) return
    const keys = {
      ArrowLeft: (index + tabs.length - 1) % tabs.length,
      ArrowRight: (index + 1) % tabs.length,
      Home: 0,
      End: tabs.length - 1,
    }
    if (!(event.key in keys)) return
    event.preventDefault()
    const next = tabs[keys[event.key]]
    next.focus()
    selectTab(tabs, next)
  })
}

async function detectSystem() {
  const ua = navigator.userAgent
  const data = navigator.userAgentData
  const platform = data?.platform || navigator.platform || ''
  if (/iPhone|iPad|iPod|Android/i.test(ua) || (/Mac/.test(platform) && navigator.maxTouchPoints > 1)) {
    return { os: 'phone' }
  }
  if (/Win/i.test(platform) || /Windows/.test(ua)) return { os: 'win' }
  if (/Mac/i.test(platform) || /Mac OS X/.test(ua)) {
    try {
      const detail = await data?.getHighEntropyValues(['architecture'])
      if (detail?.architecture) return { os: 'mac', intel: detail.architecture === 'x86', known: true }
    } catch {}
    return { os: 'mac', intel: false, known: false }
  }
  return { os: /Linux|X11/i.test(platform + ua) ? 'linux' : 'other' }
}

// Runs on the homepage (hero + install commands) and on /download/ (files, with
// the Intel/Apple silicon buttons), so each part checks that its elements exist.
detectSystem().then(({ os, intel, known }) => {
  const label = document.querySelector('[data-hero-label]')
  const note = document.querySelector('[data-hero-note]')
  const detected = document.querySelector('[data-dl-detect]')
  const version = note?.dataset.version
  if (os === 'phone' || os === 'other') {
    if (label) label.textContent = 'Get Natamux'
    if (note) note.textContent = `Version ${version} for macOS, Windows and Linux.`
    if (os === 'phone' && detected) {
      detected.textContent = 'Natamux is a desktop app. Open this page on your computer.'
      detected.hidden = false
    }
    return
  }
  const names = { mac: 'macOS', win: 'Windows', linux: 'Linux' }
  const system = { mac: known ? intel ? 'Intel Macs' : 'Apple silicon' : 'macOS', win: 'Windows 10 and 11', linux: 'Debian and Ubuntu' }[os]
  if (label) label.textContent = `Install on ${names[os]}`
  if (note) note.textContent = `Version ${version} for ${system}.`
  const silicon = document.querySelector('[data-arch="arm64"]')
  const intelLink = document.querySelector('[data-arch="x64"]')
  if (detected) {
    // Only /download/ has a chip choice; install.sh picks the chip itself.
    const guess = silicon ? 'This looks like a Mac. If it has an Intel chip, pick Intel.' : 'This looks like a Mac.'
    detected.textContent = os === 'mac'
      ? known ? `This looks like a Mac with ${intel ? 'an Intel chip' : 'Apple silicon'}.` : guess
      : `This looks like ${names[os]}.`
    detected.hidden = false
  }
  if (os === 'mac' && intel && silicon && intelLink) {
    silicon.classList.remove('primary')
    intelLink.classList.add('primary')
    intelLink.after(silicon)
  }
  const tab = document.getElementById(`tab-${os}`)
  if (tab) selectTab([...document.querySelectorAll('.dl-tabs [role="tab"]')], tab)
})

for (const button of document.querySelectorAll('[data-copy]')) {
  button.addEventListener('click', async () => {
    try {
      await navigator.clipboard.writeText(button.previousElementSibling.textContent)
      button.textContent = 'Copied'
    } catch {
      button.textContent = 'Select and copy'
    }
    setTimeout(() => { button.textContent = 'Copy' }, 1600)
  })
}
