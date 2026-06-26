import { expect, test } from '@playwright/test'

test('renders the dual-pane explorer shell', async ({ page }) => {
  await page.goto('/')
  await expect(page.getByRole('region', { name: 'Left pane' })).toBeVisible()
  await expect(page.getByRole('region', { name: 'Right pane' })).toBeVisible()
})
