// The small window a manual update check shows while it checks and downloads. No Electron, so
// tests can run it. See docs/design-desktop-update.md.
import type { BrowserWindowConstructorOptions } from 'electron'

export type PopupView = {
  kind: 'checking' | 'downloading'
  title: string
  detail: string
  hint: string
  // Undefined: the bar runs without an end.
  percent?: number
}

export const POPUP_CSP = "default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'"
const WIDTH = 360
const HEIGHT = 148
const MB = 1_000_000

export function checkingView(version: string): PopupView {
  return {
    kind: 'checking',
    title: 'Checking for updates…',
    detail: `You have Natamux ${version}`,
    hint: '',
  }
}

// total: the content-length of the download, when the server sends one.
export function downloadingView(
  version: string,
  done: number,
  total: number | undefined,
): PopupView {
  const known = total !== undefined && Number.isFinite(total) && total > 0
  // A server can send more than its content-length; the text must not pass the total.
  const shown = Math.max(0, Number.isFinite(done) ? done : 0)
  const capped = known ? Math.min(shown, total) : shown
  const mb = (bytes: number) => Math.round(bytes / MB)
  return {
    kind: 'downloading',
    title: `Downloading Natamux ${version}…`,
    detail: known ? `${mb(capped)} of ${mb(total)} MB` : `${mb(capped)} MB`,
    hint: 'You can keep working. It installs on restart.',
    ...(known ? { percent: Math.floor((capped / total) * 100) } : {}),
  }
}

// JSON keeps any text a plain value: it cannot end the call or the script.
export function showScript(view: PopupView): string {
  return `show(${JSON.stringify(view)})`
}

// Colors from packages/web/src/styles.css: popover, ink, muted, line, line-strong, accent.
const STYLE = `
:root{--bg:#ffffff;--ink:#17191d;--muted:#5f6570;--track:#eef0f3;--ring:#e3e5e9;--accent:#2f5fe0;--soft:#eef2fd;--line:#cfd3d9;--button:#3f444c;--hover:#f6f7f9}
@media (prefers-color-scheme: dark){:root{--bg:#1c1d21;--ink:#e6e7ea;--muted:#8d919a;--track:#26282d;--ring:#26282d;--accent:#7aa2ff;--soft:#232a3d;--line:#363940;--button:#b4b7be;--hover:#26282d}}
*{box-sizing:border-box}
html,body{margin:0;height:100%}
body{background:var(--bg);color:var(--ink);font-size:13px;font-family:-apple-system, "Segoe UI", system-ui, sans-serif;padding:20px;display:flex;flex-direction:column;gap:16px;-webkit-app-region:drag;user-select:none;cursor:default}
.row{display:flex;align-items:center;gap:14px}
.icon{width:40px;height:40px;flex:none;position:relative;display:flex;align-items:center;justify-content:center;border-radius:50%}
.icon svg{fill:none;stroke:var(--accent);stroke-linecap:round;stroke-linejoin:round}
.icon .track{stroke:var(--ring)}
.ring{position:absolute;inset:0;animation:spin .9s linear infinite}
.title{font-size:14px;font-weight:600}
.detail{font-size:12.5px;color:var(--muted);margin-top:3px;font-variant-numeric:tabular-nums}
.bar{height:3px;border-radius:3px;background:var(--track);overflow:hidden;position:relative}
.fill{position:absolute;top:0;left:0;height:100%;width:40%;border-radius:3px;background:var(--accent);animation:slide 1.4s cubic-bezier(.4,0,.2,1) infinite}
.bar[data-known] .fill{animation:none;transition:width .2s}
.foot{display:flex;justify-content:space-between;align-items:center;gap:12px}
.hint{font-size:12px;color:var(--muted)}
button{-webkit-app-region:no-drag;font:inherit;font-size:12.5px;color:var(--button);background:var(--bg);border:1px solid var(--line);border-radius:6px;padding:5px 14px}
button:hover{background:var(--hover)}
button:focus-visible{outline:2px solid var(--accent);outline-offset:2px}
body[data-kind=downloading] .icon{background:var(--soft)}
body[data-kind=downloading] .checking,body[data-kind=checking] .downloading{display:none}
@keyframes spin{to{transform:rotate(360deg)}}
@keyframes slide{from{transform:translateX(-100%)}to{transform:translateX(250%)}}
@media (prefers-reduced-motion: reduce){.ring,.fill{animation:none}}
`

const SCRIPT = `
const $ = (id) => document.getElementById(id)
function show(view) {
  document.body.dataset.kind = view.kind
  $('title').textContent = view.title
  $('detail').textContent = view.detail
  $('hint').textContent = view.hint
  const bar = $('bar')
  if (typeof view.percent === 'number') {
    bar.dataset.known = ''
    bar.setAttribute('aria-valuenow', String(view.percent))
    $('fill').style.width = view.percent + '%'
  } else {
    delete bar.dataset.known
    bar.removeAttribute('aria-valuenow')
    $('fill').style.width = ''
  }
}
$('hide').addEventListener('click', () => window.close())
addEventListener('keydown', (event) => {
  if (event.key === 'Escape') window.close()
})
`

const BODY = `
<div class="row">
<div class="icon" aria-hidden="true">
<svg class="ring checking" width="40" height="40" viewBox="0 0 40 40"><circle class="track" cx="20" cy="20" r="17" stroke-width="2.5"/><path d="M20 3 a17 17 0 0 1 17 17" stroke-width="2.5"/></svg>
<svg class="checking" width="18" height="18" viewBox="0 0 24 24" stroke-width="2"><path d="M21 12a9 9 0 1 1-2.64-6.36"/><path d="M21 3v6h-6"/></svg>
<svg class="downloading" width="18" height="18" viewBox="0 0 24 24" stroke-width="2"><path d="M12 4v11"/><path d="M7 10l5 5 5-5"/><path d="M5 20h14"/></svg>
</div>
<div role="status"><div class="title" id="title"></div><div class="detail" id="detail"></div></div>
</div>
<div class="bar" id="bar" role="progressbar" aria-label="Update progress" aria-valuemin="0" aria-valuemax="100"><div class="fill" id="fill"></div></div>
<div class="foot"><span class="hint" id="hint"></span><button type="button" id="hide">Hide</button></div>
`

// A data: URL needs no file in the package, as the loading page in navigation.ts.
export function updatePopupUrl(): string {
  const html = `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="${POPUP_CSP}"><title>Natamux</title><style>${STYLE}</style></head><body data-kind="checking">${BODY}<script>${SCRIPT}</script></body></html>`
  return `data:text/html;charset=utf-8,${encodeURIComponent(html)}`
}

type Bounds = { x: number; y: number; width: number; height: number }

// over: the main window. Undefined: Electron centers the popup on the screen.
export function popupWindowOptions(
  dark: boolean,
  over: Bounds | undefined,
): BrowserWindowConstructorOptions {
  return {
    width: WIDTH,
    height: HEIGHT,
    useContentSize: true,
    ...(over === undefined
      ? {}
      : {
          x: over.x + Math.round((over.width - WIDTH) / 2),
          y: over.y + Math.round((over.height - HEIGHT) / 3),
        }),
    show: false,
    frame: false,
    resizable: false,
    minimizable: false,
    maximizable: false,
    fullscreenable: false,
    skipTaskbar: true,
    title: 'Natamux',
    backgroundColor: dark ? '#1c1d21' : '#ffffff',
    webPreferences: {
      sandbox: true,
      contextIsolation: true,
      nodeIntegration: false,
      spellcheck: false,
    },
  }
}
