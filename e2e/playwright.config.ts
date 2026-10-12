import { defineConfig, devices } from '@playwright/test'

// The tests run against the production images that scripts/e2e.sh starts. See docs/e2e.md.
export default defineConfig({
  testDir: 'tests',
  // The tests share one daemon and its demo data, so they run one after the other.
  workers: 1,
  fullyParallel: false,
  retries: process.env.CI ? 1 : 0,
  // A failure ends the run: the remaining tests would only add time before the red result.
  maxFailures: process.env.CI ? 1 : 0,
  timeout: 60_000,
  expect: { timeout: 15_000 },
  reporter: process.env.CI ? [['list'], ['html', { open: 'never' }]] : 'list',
  use: {
    baseURL: process.env.E2E_URL ?? 'http://127.0.0.1:8080',
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
    video: 'retain-on-failure',
    // The terminal draws its text on a canvas with WebGL. Without WebGL it falls back to text in
    // the page, which the tests can read.
    launchOptions: { args: ['--disable-webgl', '--disable-3d-apis'] },
  },
  projects: [
    {
      name: 'desktop',
      use: { ...devices['Desktop Chrome'], viewport: { width: 1440, height: 900 } },
      testIgnore: /phone\.spec\.ts/,
    },
    {
      name: 'phone',
      use: { ...devices['Pixel 7'] },
      testMatch: /phone\.spec\.ts/,
    },
  ],
})
