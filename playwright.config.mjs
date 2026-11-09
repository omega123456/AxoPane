import os from 'node:os'
import { defineConfig, devices } from '@playwright/test'

const port = Number(process.env.PLAYWRIGHT_PORT ?? '4173')
const baseURL = `http://127.0.0.1:${port}`

const availableCpus = Math.max(1, os.availableParallelism?.() ?? os.cpus().length)
const isCI = !!process.env.CI
const workers = Math.min(isCI ? 2 : 9, availableCpus)

export default defineConfig({
  testDir: './e2e',
  fullyParallel: true,
  workers,
  globalSetup: './e2e/global-setup.ts',
  reporter: 'list',
  use: {
    baseURL,
    trace: 'on-first-retry',
    // Pin the browser timezone so local-time date formatting renders
    // deterministically regardless of the host machine's timezone.
    timezoneId: 'UTC',
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],
  webServer: {
    command: `pnpm exec vite --host 127.0.0.1 --port ${port}`,
    url: baseURL,
    reuseExistingServer: !process.env.CI,
    env: {
      VITE_PLAYWRIGHT: 'true',
      PLAYWRIGHT_PORT: String(port),
    },
  },
})
