const root = document.documentElement
const colorScheme = matchMedia('(prefers-color-scheme: light)')
const themeToggle = document.querySelector('[data-theme-toggle]')
let followsSystem = true
try {
  followsSystem = !['light', 'dark'].includes(localStorage.getItem('starlight-theme'))
} catch {}

function labelTheme() {
  const next = root.dataset.theme === 'light' ? 'dark' : 'light'
  themeToggle?.setAttribute('aria-label', `Switch to ${next} theme`)
}
themeToggle?.addEventListener('click', () => {
  root.dataset.theme = root.dataset.theme === 'light' ? 'dark' : 'light'
  followsSystem = false
  try {
    localStorage.setItem('starlight-theme', root.dataset.theme)
  } catch {}
  labelTheme()
})
colorScheme.addEventListener('change', () => {
  if (followsSystem) {
    root.dataset.theme = colorScheme.matches ? 'light' : 'dark'
    labelTheme()
  }
})
labelTheme()

