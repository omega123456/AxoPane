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
    await page.goto(baseURL, { waitUntil: 'networkidle' })
    await page.getByRole('region', { name: 'Left pane' }).waitFor()
    await page.close()
  } finally {
    await browser.close()
  }
}
