import { expect, test } from '@playwright/test'
import { gotoScenario, openSettingsSection, rightClickPane } from './helpers'
import { screenshotScenarios } from '../src/tests/playwright-fixtures/e2e'
import { expandedQueueFinalProgressEvent } from '../src/tests/playwright-fixtures/queue'

for (const mode of ['light', 'dark'] as const) {
  test(`browsing ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.browsing[mode])
    await expect(page.getByRole('region', { name: 'Left pane' })).toBeVisible()
    await expect(page.getByRole('row', { name: /Documents/ }).first()).toBeVisible()
    await expect(page.locator('main')).toHaveScreenshot(`dual-pane-browsing-${mode}.png`)
  })

  test(`file types ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.fileTypes[mode])
    await expect(page.getByRole('region', { name: 'Left pane' })).toBeVisible()
    await expect(page.getByRole('row', { name: /installer\.exe/ }).first()).toBeVisible()
    await expect(page.locator('main')).toHaveScreenshot(`mixed-file-types-${mode}.png`)
  })

  test(`loading ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.loading[mode])
    await expect(page.getByRole('status', { name: 'Loading folder' }).first()).toBeVisible()
    await expect(page.locator('main')).toHaveScreenshot(`folder-loading-state-${mode}.png`)
  })

  test(`empty ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.empty[mode])
    await expect(page.getByText('This folder is empty').first()).toBeVisible()
    await expect(page.locator('main')).toHaveScreenshot(`empty-folder-state-${mode}.png`)
  })

  test(`error ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.error[mode])
    await expect(page.getByText('Directory refresh failed: device timeout.').first()).toBeVisible()
    await expect(page.locator('main')).toHaveScreenshot(`directory-error-state-${mode}.png`)
  })

  test(`permission ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.permission[mode])
    await expect(page.getByText('You do not have permission to view this folder.').first()).toBeVisible()
    await expect(page.locator('main')).toHaveScreenshot(`permission-denied-state-${mode}.png`)
  })

  test(`size states ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.sizes[mode])
    await expect(page.getByText('941.9 MB').first()).toBeVisible()
    await expect(page.getByRole('row', { name: /Documents/ }).first()).toBeVisible()
    await expect(page.locator('main')).toHaveScreenshot(`folder-size-states-${mode}.png`)
  })

  test(`queue collapsed ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.queueCollapsed[mode])
    await expect(page.getByRole('button', { name: 'Expand transfer queue' })).toBeVisible()
    await expect(page.locator('main')).toHaveScreenshot(`transfer-queue-collapsed-${mode}.png`)
  })

  test(`queue expanded ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.queueExpanded[mode])
    await page.getByRole('button', { name: 'Expand transfer queue' }).click()
    await expect(page.getByRole('region', { name: 'Transfer queue' })).toBeVisible()
    await expect(
      page.getByRole('progressbar', {
        name: `Copying ${expandedQueueFinalProgressEvent.totalItems.toLocaleString()} items`,
      }),
    ).toHaveAttribute('aria-valuenow', String(Math.round(expandedQueueFinalProgressEvent.progressPercent)))
    // The chart is throttled, so wait until its filled extent reaches the final
    // seeded percent before capturing (condition-based, no fixed delay).
    await expect(page.locator('[data-testid="throughput-chart-fill-extent"]').first()).toHaveAttribute(
      'width',
      String(expandedQueueFinalProgressEvent.progressPercent),
    )
    await expect(page.locator('main')).toHaveScreenshot(`transfer-queue-expanded-${mode}.png`)
  })

  test(`conflict modal ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.conflict[mode])
    await expect(page.getByRole('dialog', { name: 'Resolve file conflict' })).toBeVisible()
    await expect(page.locator('main')).toHaveScreenshot(`file-conflict-dialog-${mode}.png`)
  })

  test(`settings keybindings ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.browsing[mode])
    await openSettingsSection(page, 'keybindings')
    await expect(page.getByLabel('Search keybindings')).toBeVisible()
    await expect(page.locator('main')).toHaveScreenshot(`settings-keybindings-${mode}.png`)
  })

  test(`settings columns ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.browsing[mode])
    await openSettingsSection(page, 'columns')
    await expect(page.getByRole('switch', { name: 'Created column' })).toBeVisible()
    await expect(page.locator('main')).toHaveScreenshot(`settings-columns-${mode}.png`)
  })

  test(`settings layout ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.browsing[mode])
    await openSettingsSection(page, 'layout')
    await expect(page.getByText('Default pane mode')).toBeVisible()
    await expect(page.locator('main')).toHaveScreenshot(`settings-layout-${mode}.png`)
  })

  test(`settings updates ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.browsing[mode])
    await openSettingsSection(page, 'updates')
    await expect(page.getByLabel('Update check frequency')).toBeVisible()
    await expect(page.locator('main')).toHaveScreenshot(`settings-updates-${mode}.png`)
  })

  test(`pane context menu ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.browsing[mode])
    await rightClickPane(page, 'Left pane')
    await expect(page.getByRole('menu', { name: 'This folder' })).toBeVisible()
    await expect(page.locator('main')).toHaveScreenshot(`pane-context-menu-${mode}.png`)
  })

  test(`row context menu ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.browsing[mode])
    await page.getByRole('row', { name: /Documents/ }).first().click({ button: 'right' })
    await expect(page.getByRole('menu', { name: 'Documents' })).toBeVisible()
    await expect(page.locator('main')).toHaveScreenshot(`row-context-menu-${mode}.png`)
  })

  test(`new folder dialog ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.browsing[mode])
    await rightClickPane(page, 'Left pane')
    await page.getByRole('menuitem', { name: 'New folder' }).click()
    await expect(page.getByRole('dialog', { name: 'New folder' })).toBeVisible()
    await expect(page.locator('main')).toHaveScreenshot(`new-folder-dialog-${mode}.png`)
  })

  test(`rename inline ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.browsing[mode])
    await page.getByRole('row', { name: /Documents/ }).first().click({ button: 'right' })
    const menu = page.getByRole('menu', { name: 'Documents' })
    await expect(menu).toBeVisible()
    await menu.getByRole('menuitem').filter({ hasText: 'Rename' }).click()
    await expect(page.getByRole('textbox', { name: /Rename Documents/ })).toBeVisible()
    await expect(page.locator('main')).toHaveScreenshot(`rename-inline-editor-${mode}.png`)
  })

  test(`delete dialog ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.browsing[mode])
    await page.getByRole('row', { name: /Documents/ }).first().click({ button: 'right' })
    const menu = page.getByRole('menu', { name: 'Documents' })
    await expect(menu).toBeVisible()
    await menu.getByRole('menuitem').filter({ hasText: 'Delete' }).click()
    await expect(page.getByRole('dialog', { name: 'Confirm delete' })).toBeVisible()
    await expect(page.locator('main')).toHaveScreenshot(`delete-confirmation-dialog-${mode}.png`)
  })
}
