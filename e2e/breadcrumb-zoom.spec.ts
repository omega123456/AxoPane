import { expect, test } from '@playwright/test'
import { gotoScenario } from './helpers'
import { screenshotScenarios } from '../src/tests/playwright-fixtures/e2e'

// The pane whose crumbs have to shorten differs per zoom; at 125% the left
// pane is too narrow for anything but a clipped current folder.
for (const [zoom, scenario, collapsedPane] of [
  ['125', screenshotScenarios.breadcrumbsZoomed.light, 'Right pane'],
  ['80', screenshotScenarios.breadcrumbsZoomed.dark, 'Left pane'],
] as const) {
  test(`fits the breadcrumbs inside the address bar at ${zoom}% app zoom`, async ({ page }) => {
    await page.setViewportSize({ width: 720, height: 520 })
    await gotoScenario(page, scenario)
    const pane = page.getByRole('region', { name: collapsedPane })
    await expect(pane.getByTestId('breadcrumb-collapse-marker')).toBeVisible()
    const nav = pane.getByRole('navigation', { name: `${collapsedPane} path` })
    const current = nav.locator('[aria-current="location"]')
    await expect(current).toHaveText('Foxtrot')
    const navBox = await nav.boundingBox()
    const currentBox = await current.boundingBox()
    if (!navBox || !currentBox) throw new Error('missing breadcrumb geometry')
    expect(currentBox.x + currentBox.width).toBeLessThanOrEqual(navBox.x + navBox.width + 0.5)
  })
}
