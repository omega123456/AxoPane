import { expect, test } from '@playwright/test'
import { gotoScenario } from './helpers'
import { screenshotScenarios } from '../src/tests/playwright-fixtures/e2e'

for (const [zoom, scenario] of [
  ['150', screenshotScenarios.tabDragZoomed.light],
  ['80', screenshotScenarios.tabDragZoomed.dark],
] as const) {
  test(`keeps a dragged tab under the pointer at ${zoom}% app zoom`, async ({ page }) => {
    await gotoScenario(page, scenario)
    const tabs = page.getByRole('region', { name: 'Left pane' }).getByRole('tab')
    const source = tabs.nth(0)
    const destinationTabs =
      zoom === '150' ? tabs : page.getByRole('region', { name: 'Right pane' }).getByRole('tab')
    const target = zoom === '150' ? tabs.nth(2) : destinationTabs.last()
    const sourceBox = await source.locator('..').boundingBox()
    const targetBox = await target.boundingBox()
    if (!sourceBox || !targetBox) throw new Error('missing tab geometry')

    const start = {
      x: sourceBox.x + sourceBox.width / 2,
      y: sourceBox.y + sourceBox.height / 2,
    }
    const end = {
      x: targetBox.x + targetBox.width / 2,
      y: targetBox.y + targetBox.height / 2,
    }
    await page.mouse.move(start.x, start.y)
    await page.mouse.down()
    await page.mouse.move(end.x, end.y, { steps: 6 })

    await expect
      .poll(async () => {
        const box = await page.locator('[data-dnd-dragging]').boundingBox()
        if (!box) return Number.POSITIVE_INFINITY
        return Math.hypot(box.x + box.width / 2 - end.x, box.y + box.height / 2 - end.y)
      })
      .toBeLessThan(1)

    await page.mouse.up()
    if (zoom === '150') {
      await expect(tabs.last()).toHaveText('Omega')
    } else {
      await expect(tabs).toHaveCount(2)
      await expect(destinationTabs).toHaveCount(3)
    }
    await expect(page.locator('[data-tab-id][style*="zoom"]')).toHaveCount(0)
  })
}
