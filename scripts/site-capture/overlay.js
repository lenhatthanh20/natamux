// Demo overlay for recordings: a large cursor that follows the mouse, a ripple on each click, and a
// Screen Studio style focus zoom. Add with page.addInitScript({ path }) before the first goto.
// API in the page: __demo.focus(selectorOrRect, scale, ms), __demo.unfocus(ms), __demo.caption(text|null).
(() => {
  const ZOOM = Number(localStorage.getItem('demo.zoom') || 1); // 2 = crisp 2x video, see capture.md
  const install = () => {
    if (window.__demo) return;
    document.documentElement.style.zoom = String(ZOOM);
    const css = document.createElement('style');
    css.textContent = `
      #demo-cursor { position: fixed; left: 0; top: 0; width: 30px; height: 30px; z-index: 2147483647;
        pointer-events: none; transform: translate(-3px, -2px); transition: transform 60ms linear;
        filter: drop-shadow(0 2px 6px rgba(0,0,0,.45)); }
      #demo-cursor.down svg { transform: scale(.86); transform-origin: 3px 2px; }
      .demo-ripple { position: fixed; z-index: 2147483646; pointer-events: none; width: 16px; height: 16px;
        margin: -8px 0 0 -8px; border-radius: 50%; border: 3px solid rgba(255, 196, 64, .95);
        background: rgba(255, 196, 64, .25); animation: demo-ripple 650ms ease-out forwards; }
      @keyframes demo-ripple { to { transform: scale(4.2); opacity: 0; } }
      body.demo-zoomable { transition: transform var(--demo-ms, 700ms) cubic-bezier(.22, .8, .2, 1); will-change: transform; }
      #demo-caption { position: fixed; left: 50%; bottom: 28px; transform: translateX(-50%); z-index: 2147483645;
        pointer-events: none; padding: 10px 18px; border-radius: 12px; font: 600 17px/1.3 Inter, system-ui, sans-serif;
        color: #fff; background: rgba(12, 14, 20, .82); box-shadow: 0 8px 30px rgba(0,0,0,.35);
        backdrop-filter: blur(8px); opacity: 0; transition: opacity 250ms; }
      #demo-caption.on { opacity: 1; }`;
    document.head.append(css);
    const cursor = document.createElement('div');
    cursor.id = 'demo-cursor';
    cursor.innerHTML = `<svg width="30" height="30" viewBox="0 0 24 24"><path d="M3 2l7.5 19 2.6-7.9L21 10.5z"
      fill="#fff" stroke="#111" stroke-width="1.6" stroke-linejoin="round"/></svg>`;
    const caption = document.createElement('div');
    caption.id = 'demo-caption';
    // On <html>, outside <body>, so the focus zoom of <body> does not move them.
    document.documentElement.append(cursor, caption);
    document.body.classList.add('demo-zoomable');
    // Under html zoom, event and rect coordinates are viewport pixels, while CSS lengths of zoomed
    // elements are multiplied by the zoom. So divide by it.
    const at = (e) => [e.clientX / ZOOM, e.clientY / ZOOM];
    addEventListener('mousemove', (e) => {
      const [x, y] = at(e);
      cursor.style.transform = `translate(${x - 3}px, ${y - 2}px)`;
    }, true);
    addEventListener('mousedown', (e) => {
      const [x, y] = at(e);
      cursor.classList.add('down');
      const ring = document.createElement('div');
      ring.className = 'demo-ripple';
      ring.style.left = `${x}px`;
      ring.style.top = `${y}px`;
      document.documentElement.append(ring);
      setTimeout(() => ring.remove(), 700);
    }, true);
    addEventListener('mouseup', () => cursor.classList.remove('down'), true);
    const rectOf = (target) => typeof target === 'string'
      ? document.querySelector(target)?.getBoundingClientRect() : target;
    window.__demo = {
      focus(target, scale = 1.6, ms = 700) {
        const r = rectOf(target);
        if (!r) return false;
        const cx = (r.left + r.width / 2) / ZOOM, cy = (r.top + r.height / 2) / ZOOM;
        document.body.style.setProperty('--demo-ms', `${ms}ms`);
        document.body.style.transformOrigin = `${cx}px ${cy}px`;
        document.body.style.transform = `scale(${scale})`;
        return true;
      },
      unfocus(ms = 700) {
        document.body.style.setProperty('--demo-ms', `${ms}ms`);
        document.body.style.transform = '';
      },
      caption(text) {
        caption.textContent = text ?? '';
        caption.classList.toggle('on', Boolean(text));
      },
    };
  };
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', install);
  else install();
})();
