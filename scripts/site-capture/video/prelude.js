async page => {
  const OUT = '__OUT__';
  const LEAD = __LEAD__; // the lead session makes the real state changes; the other one only mirrors them
  const API = async (method, path, body) => page.evaluate(async ([m, p, b]) => {
    const r = await fetch('/api' + p, { method: m, headers: { authorization: 'Bearer e2e-token', 'content-type': 'application/json' }, body: b ? JSON.stringify(b) : undefined });
    return { status: r.status, body: await r.json().catch(() => null) };
  }, [method, path, body]);
  const sleep = (ms) => page.waitForTimeout(ms);
  let pos = [720, 450];
  const glide = async (x, y, ms = 900) => { await page.mouse.move(x, y, { steps: Math.max(2, Math.round(ms / 16)) }); pos = [x, y]; };
  const bbox = async (loc) => { await loc.waitFor({ timeout: 120000 }); return await loc.boundingBox(); };
  const to = async (loc, ms = 900, dx = 0, dy = 0) => { const b = await bbox(loc); await glide(b.x + b.width / 2 + dx, b.y + b.height / 2 + dy, ms); return b; };
  const press = async (real = true) => {
    if (real) { await page.mouse.down(); await sleep(90); await page.mouse.up(); return; }
    await page.evaluate(([x, y]) => dispatchEvent(new MouseEvent('mousedown', { clientX: x, clientY: y })), pos);
    await sleep(90);
    await page.evaluate(([x, y]) => dispatchEvent(new MouseEvent('mouseup', { clientX: x, clientY: y })), pos);
  };
  const cap = (t) => page.evaluate((t) => __demo.caption(t), t);
  const focus = (b, s = 1.5, ms = 800) => page.evaluate(([b, s, ms]) => __demo.focus({ left: b.x, top: b.y, width: b.width, height: b.height }, s, ms), [b, s, ms]);
  const unfocus = (ms = 800) => page.evaluate((ms) => __demo.unfocus(ms), ms);
  const smooth = (sel, by, ms) => page.evaluate(([sel, by, ms]) => new Promise((done) => {
    const el = typeof sel === 'string' ? document.querySelector(sel) : sel;
    const from = el.scrollTop, t0 = performance.now();
    const step = (t) => { const k = Math.min(1, (t - t0) / ms), e = k < .5 ? 2 * k * k : 1 - (-2 * k + 2) ** 2 / 2;
      el.scrollTop = from + by * e; k < 1 ? requestAnimationFrame(step) : done(); };
    requestAnimationFrame(step);
  }), [sel, by, ms]);
  const notips = () => page.addStyleTag({ content: '[role=tooltip]{display:none!important} html{overflow:hidden!important}' });
  const marks = [];
  let t0 = 0;
  const mark = (name) => marks.push([name, (Date.now() - t0) / 1000]);
  const rec = async () => { await page.screencast.start({ path: OUT, size: { width: 2880, height: 1800 }, fps: 30 }); t0 = Date.now(); };
  const stop = async () => { mark('end'); await page.screencast.stop(); return JSON.stringify(marks); };
  try {
  // __BODY__
  } catch (e) { try { await page.screencast.stop(); } catch {} return 'ERR ' + e.message + ' ' + JSON.stringify(marks); }
}
