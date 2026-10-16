async page => {
  await page.addInitScript({ path: '__OVERLAY__' });
  await page.evaluate(() => { localStorage.setItem('natamux.sidebar-folds', JSON.stringify({'repo:default': false})); localStorage.removeItem('demo.zoom'); });
  await page.goto('http://localhost:__PORT__/w/__TASK__');
  await page.waitForTimeout(1500);
  return await page.evaluate(() => [!!window.__demo, document.documentElement.style.zoom, innerWidth, devicePixelRatio, matchMedia('(prefers-color-scheme: dark)').matches]);
}
