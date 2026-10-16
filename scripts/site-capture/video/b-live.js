  // film chapters 2, 3, 4 and 6 in one take of a fresh bug task: approve Spec, Took Bug fix, side by side,
  // Review sends it back, Manual review approve. The lead session clicks for real, the other mirrors.
  const ID = '__TASK__';
  await page.goto('http://localhost:8090/w/' + ID);
  const approve = page.getByRole('button', { name: 'Approve', exact: true }).first();
  await approve.waitFor();
  await notips();
  const panel = 'aside[aria-label^="Details of"] .overflow-y-auto';
  const stageNow = async () => (await API('GET', '/tasks/' + ID)).body.task;
  const until = async (ok, ms = 240000) => {
    const end = Date.now() + ms;
    for (;;) { const t = await stageNow(); if (ok(t)) return t; if (Date.now() > end) throw new Error('timeout ' + JSON.stringify([t.state, t.stage])); await sleep(250); }
  };
  // scroll the Progress panel so the element with this text sits at `at` (0..1) of the panel height
  const show = async (re, at = 0.3, ms = 900) => {
    const el = page.locator('aside[aria-label^="Details of"]').getByText(re).first();
    await el.waitFor({ timeout: 120000 });
    const b = await el.boundingBox(), s = await page.locator(panel).boundingBox();
    await smooth(panel, b.y - (s.y + s.height * at), ms);
  };
  const shells = async () => page.locator('main').getByText(/^Shell \d+$/).evaluateAll((els) => els.map((e) => { const r = e.getBoundingClientRect(); return [e.textContent, r.x + r.width / 2, r.y + r.height / 2]; }));
  await page.mouse.move(700, 560); pos = [700, 560];
  await sleep(1500);
  await rec();
  await sleep(1200);
  // chapter 2: approve the Spec, the branch takes Bug fix
  await to(approve, 1100);
  await sleep(250);
  await press(LEAD);
  mark('approve');
  await sleep(500);
  await glide(1260, 420, 900);
  await show(/^Took Bug fix/, 0.28, 900);
  mark('took');
  await cap('The branch took Bug fix');
  await sleep(2600);
  await cap(null);
  // chapter 3: Gemini (Test spec) and Claude Code (Implement) side by side
  await until((t) => ['build', 'test-spec', 'implement'].includes(t.stage));
  mark('build');
  await show(/^Test spec and Implement$/, 0.15, 900);
  await sleep(300);
  let sh = await shells();
  const s1 = sh.find((s) => s[0] === 'Shell 1');
  const last = sh[sh.length - 1];
  await cap('Two agents work side by side');
  if (last) await glide(last[1] + 60, last[2] + 140, 1100);
  await sleep(1500);
  if (s1) await glide(s1[1] + 80, s1[2] + 160, 1100);
  await sleep(1500);
  await cap(null);
  // chapter 4: Codex review sends it back, Implement runs again
  await until((t) => t.stage === 'agent-review');
  mark('review');
  await show(/^Review$/, 0.35, 900);
  sh = await shells();
  const rv = sh[sh.length - 1];
  if (rv) await glide(rv[1] + 120, rv[2] + 200, 1100);
  await until((t) => t.stage !== 'agent-review', 120000);
  mark('sentback');
  const rb = await page.locator('aside[aria-label^="Details of"]').getByText(/^Review$/).first().boundingBox();
  await focus({ x: 1439, y: rb.y, width: 2, height: 2 }, 1.4, 900);
  await cap('Review sent it back to Implement');
  await glide(1220, rb.y + 10, 900);
  await sleep(2600);
  await unfocus(800);
  await cap(null);
  // chapter 6: Manual review waits for you
  await until((t) => t.stage === 'review' && t.state === 'review', 240000);
  mark('manual');
  await show(/^Manual review$/, 0.3, 900);
  const ap = page.getByRole('button', { name: 'Approve', exact: true }).first();
  await sleep(900);
  await to(ap, 1100);
  await sleep(300);
  await press(LEAD);
  mark('approved');
  await sleep(1800);
  return await stop();
