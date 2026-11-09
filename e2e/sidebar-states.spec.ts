import { expect, test } from '@playwright/test'
import { gotoScenario } from './helpers'
import { screenshotScenarios } from '../src/tests/playwright-fixtures/e2e'

test('deep path levels fold into one row', async ({ page }) => {
  await gotoScenario(page, screenshotScenarios.deepTree.light)
  const treeScroll = page.getByTestId('folder-tree-scroll')
  await expect(treeScroll.getByRole('button', { name: 'out', exact: true })).toBeVisible()
  await expect(treeScroll.getByRole('button', { name: '4 levels hidden' })).toBeVisible()
})

test('tree context menu offers both panes', async ({ page }) => {
  await gotoScenario(page, screenshotScenarios.stickyTree.light)
  await page
    .getByTestId('folder-tree-scroll')
    .getByRole('button', { name: 'Designs', exact: true })
    .click({ button: 'right' })
  const menu = page.getByRole('menu', { name: 'Designs' })
  await expect(menu.getByRole('menuitem', { name: 'Open in right pane' })).toBeVisible()
  await expect(menu.getByRole('menuitem', { name: 'Open in left pane' })).toBeVisible()
})

test('collapsed favourites keep the tree visible', async ({ page }) => {
  await gotoScenario(page, screenshotScenarios.favouritesCollapsed.light)
  const heading = page.getByTestId('favourites-panel').getByRole('button', { name: /Favourites/ })
  await expect(heading).toHaveAttribute('aria-expanded', 'false')
  await expect(
    page.getByTestId('folder-tree-scroll').getByRole('button', { name: 'Designs', exact: true }),
  ).toBeVisible()
})

test('favourites reorder by drag', async ({ page }) => {
  await gotoScenario(page, screenshotScenarios.favourites.light)
  const panel = page.getByTestId('favourites-panel')
  const source = panel.getByRole('button', { name: 'E:', exact: true }).locator('..')
  const target = panel.getByRole('button', { name: 'Documents', exact: true }).first().locator('..')
  await source.dragTo(target)
  await expect(panel.locator('[data-favourite-row]').first()).toContainText('E:')
})

test('Everything banner shows on Windows when Everything is down', async ({ page }) => {
  await gotoScenario(page, screenshotScenarios.everythingBanner.light)
  await expect(page.getByRole('status', { name: 'Everything unavailable' })).toBeVisible()
})
