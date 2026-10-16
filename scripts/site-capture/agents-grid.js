// Shows only the given tasks in the Agents grid (the choice lives in memory until a reload).
async page => {
  const keep = ['T-0011', 'T-0012', 'T-0013', 'T-0014', 'T-0015'];
  await page.goto(new URL('/agents', page.url()).href);
  await page.getByRole('heading', { name: 'Agents' }).waitFor();
  const shownIds = () => page.getByRole('button', { name: /^Close Agent of T-\d+$/ })
    .evaluateAll(b => b.map(x => x.getAttribute('aria-label').split(' ').pop()));
  for (let drop; (drop = (await shownIds()).find(id => !keep.includes(id))); ) {
    await page.getByRole('button', { name: `Close Agent of ${drop}` }).click();
  }
  for (const id of keep) {
    if (await page.getByRole('button', { name: `Close Agent of ${id}` }).count()) continue;
    const box = page.getByRole('combobox', { name: 'Add a pane' });
    await box.fill(id);
    await page.getByRole('option', { name: new RegExp(id) }).first().click();
  }
  return await page.getByRole('button', { name: /^Close Agent of / }).evaluateAll(b => b.map(x => x.getAttribute('aria-label')));
}
