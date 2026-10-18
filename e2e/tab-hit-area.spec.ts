import { expect, test } from '@playwright/test'
import { gotoScenario } from './helpers'
import { screenshotScenarios } from '../src/tests/playwright-fixtures/e2e'

test('switches tabs from anywhere on the tab, not just its label', async ({ page }) => {
  await gotoScenario(page, screenshotScenarios.tabs.light)
  const tabs = page.getByRole('group', { name: 'Left pane tabs' }).getByRole('tab')
  await expect(tabs).toHaveCount(4)

  // Each point is inside the tab's outline but outside the label's own box:
  // the top-left corner, the bottom-left corner and the right-hand padding.
  for (const [index, corner] of [
    [1, 'top-left'],
    [2, 'bottom-left'],
    [3, 'right-edge'],
  ] as const) {
    const box = await tabs.nth(index).locator('..').boundingBox()
    if (!box) throw new Error('missing tab geometry')
    const point =
      corner === 'top-left'
        ? { x: box.x + 2, y: box.y + 2 }
        : corner === 'bottom-left'
          ? { x: box.x + 2, y: box.y + box.height - 2 }
          : { x: box.x + box.width - 2, y: box.y + box.height / 2 }
    await page.mouse.click(point.x, point.y)
    await expect(tabs.nth(index)).toHaveAttribute('aria-selected', 'true')
  }
})
