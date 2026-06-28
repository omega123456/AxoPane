import { chromium, type FullConfig } from '@playwright/test'

export default async function globalSetup(config: FullConfig) {
  const project = config.projects[0]
  const baseURL = project?.use?.baseURL

  if (typeof baseURL !== 'string' || baseURL.length === 0) {
    return
  }

  const browser = await chromium.launch()
  try {
    const page = await browser.newPage()
    try {
      await page.goto(baseURL, { waitUntil: 'domcontentloaded', timeout: 10_000 })
      await page.getByRole('region', { name: 'Left pane' }).waitFor({ timeout: 10_000 })
    } catch {
      // Best-effort warmup only. Playwright's webServer readiness and the specs
      // themselves provide the real pass/fail signal.
    }
    await page.close()
  } finally {
    await browser.close()
  }
}
