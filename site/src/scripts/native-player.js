const root = document.documentElement
const motion = matchMedia('(prefers-reduced-motion: reduce)')

// Keep the hidden theme idle, and load footage only when its player approaches the viewport.
const players = [...document.querySelectorAll('[data-native-player]')].map((box) => {
  const player = {
    box,
    videos: [...box.querySelectorAll('video')],
    button: box.querySelector('[data-native-toggle]'),
    label: box.querySelector('[data-native-label]'),
    wanted: !motion.matches,
    near: false,
    visible: false,
    shown: null,
  }
  box.querySelector('[data-native-controls]').hidden = false
  for (const video of player.videos) {
    video.controls = false
    video.addEventListener('playing', () => renderPlayer(player))
    video.addEventListener('pause', () => renderPlayer(player))
    video.addEventListener('error', () => {
      if (video !== player.shown) return
      player.wanted = false
      renderPlayer(player)
    })
  }
  player.button.addEventListener('click', () => {
    player.wanted = player.shown?.paused ?? true
    player.near = true
    player.visible = true
    syncPlayer(player)
  })
  return player
})

function renderPlayer(player) {
  const playing = player.shown && !player.shown.paused
  player.label.textContent = playing ? 'Pause' : 'Play'
}

function syncPlayer(player) {
  const theme = root.dataset.theme === 'light' ? 'light' : 'dark'
  const video = player.videos.find((item) => item.classList.contains(`for-${theme}`))
  if (player.shown !== video) {
    const time = player.shown?.currentTime ?? 0
    player.shown?.pause()
    player.shown = video
    const seek = () => {
      if (Number.isFinite(video.duration) && video.duration > 0) {
        video.currentTime = time % video.duration
      }
    }
    if (video.readyState) seek()
    else video.addEventListener('loadedmetadata', seek, { once: true })
  }
  if (player.near) {
    if (!video.poster) video.poster = video.dataset.poster
    const source = video.querySelector('source')
    if (!source.getAttribute('src')) {
      source.src = source.dataset.src
      video.load()
    }
  }
  if (player.wanted && player.near && player.visible && !document.hidden) {
    video.play().catch((error) => {
      if (error.name === 'AbortError') return
      player.wanted = false
      renderPlayer(player)
    })
  } else video.pause()
  renderPlayer(player)
}

const nearViewport = new IntersectionObserver((entries) => {
  for (const entry of entries) {
    if (!entry.isIntersecting) continue
    const player = players.find((item) => item.box === entry.target)
    player.near = true
    nearViewport.unobserve(player.box)
    syncPlayer(player)
  }
}, { rootMargin: '400px 0px' })
const inViewport = new IntersectionObserver((entries) => {
  for (const entry of entries) {
    const player = players.find((item) => item.box === entry.target)
    player.visible = entry.isIntersecting && entry.intersectionRatio >= 0.25
    syncPlayer(player)
  }
}, { threshold: [0, 0.25] })
for (const player of players) {
  nearViewport.observe(player.box)
  inViewport.observe(player.box)
}
const syncPlayers = () => players.forEach(syncPlayer)
document.addEventListener('visibilitychange', syncPlayers)
new MutationObserver(() => {
  syncPlayers()
}).observe(root, { attributes: true, attributeFilter: ['data-theme'] })
motion.addEventListener('change', () => {
  if (motion.matches) {
    for (const player of players) player.wanted = false
    syncPlayers()
  }
})

