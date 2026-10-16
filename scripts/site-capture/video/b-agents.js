  // clip-agents and film chapter 5: the Agents view with five providers at work; the cursor glides over pane headers.
  const keep = '__IDS__'.split(' ');
  await page.goto('http://localhost:8090/agents');
  await page.getByRole('heading', { name: 'Agents' }).waitFor();
  await notips();
  const shownIds = () => page.getByRole('button', { name: /^Close Agent of T-\d+$/ })
    .evaluateAll((b) => b.map((x) => x.getAttribute('aria-label').split(' ').pop()));
  for (let drop; (drop = (await shownIds()).find((id) => !keep.includes(id))); ) {
    await page.getByRole('button', { name: `Close Agent of ${drop}` }).click();
  }
  for (const id of keep) {
    if (await page.getByRole('button', { name: `Close Agent of ${id}` }).count()) continue;
    const box = page.getByRole('combobox', { name: 'Add a pane' });
    await box.fill(id);
    await page.getByRole('option', { name: new RegExp(id) }).first().click();
  }
  await page.mouse.move(700, 120); pos = [700, 120];
  await page.keyboard.press('Escape');
  await sleep(1500);
  // pane header spots, in reading order
  const heads = await page.getByRole('button', { name: /^Close Agent of / }).evaluateAll((bs) => bs.map((b) => {
    let p = b; while (p && p.getBoundingClientRect().width < 300) p = p.parentElement;
    const r = p.getBoundingClientRect(), h = b.getBoundingClientRect();
    return [r.x + 130, h.y + h.height / 2];
  }).sort((a, b) => a[1] - b[1] || a[0] - b[0]));
  await rec();
  await sleep(700);
  for (const [x, y] of heads) { await glide(x, y, 900); await sleep(650); }
  await glide(720, 520, 1000);
  await sleep(1800);
  return await stop();
