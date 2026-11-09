import { expect, test } from '@playwright/test'
import { gotoScenario, openSettingsSection } from './helpers'
import { screenshotScenarios } from '../src/tests/playwright-fixtures/e2e'
import { macIndexStatuses, noFolderSizeBackend } from '../src/tests/playwright-fixtures/states'
import { folderSizeStatusLabel } from '../src/lib/folder-size-status'

const browsing = screenshotScenarios.browsing.light

test('layout hides the auto folder size toggle without a backend', async ({ page }) => {
  await gotoScenario(page, {
    ...browsing,
    platform: 'macos',
    commands: { ...browsing.commands, folder_size_index_status: noFolderSizeBackend },
  })
  await openSettingsSection(page, 'layout')
  await expect(page.getByText('Default pane mode')).toBeVisible()
  await expect(
    page.getByRole('switch', { name: 'Automatically calculate folder sizes' }),
  ).toHaveCount(0)
})

for (const [name, status] of Object.entries(macIndexStatuses)) {
  test(`auto folder size row on macOS reports ${name}`, async ({ page }) => {
    await gotoScenario(page, {
      ...browsing,
      platform: 'macos',
      commands: { ...browsing.commands, folder_size_index_status: status },
    })
    await openSettingsSection(page, 'searchIndex')
    const row = page
      .getByText('Automatically calculate folder sizes', { exact: true })
      .locator('xpath=../..')
    await expect(row.getByText(/Folders you open are sized right away/)).toBeVisible()
    await expect(row.getByRole('status')).toHaveText(folderSizeStatusLabel(status))
  })
}

test('reset expander keeps the macOS index exclusions', async ({ page }) => {
  await gotoScenario(page, screenshotScenarios.searchIndexEmpty.light)
  await openSettingsSection(page, 'dates')
  const expander = page.getByRole('button', { name: /Reset to defaults/ })
  await expander.click()
  await expect(expander).toHaveAttribute('aria-expanded', 'true')
  await expect(expander).toContainText('Search index exclusions stay as they are.')
})

test('search index with no exclusions', async ({ page }) => {
  await gotoScenario(page, screenshotScenarios.searchIndexEmpty.light)
  await openSettingsSection(page, 'searchIndex')
  await expect(page.getByText('Everything is indexed')).toBeVisible()
})

test('search index after removing an exclusion', async ({ page }) => {
  await gotoScenario(page, screenshotScenarios.searchIndexAfterRemoval.light)
  await openSettingsSection(page, 'searchIndex')
  await page
    .getByRole('button', { name: 'Remove exclusion for /Users/alex/Projects/app/build-cache' })
    .click()
  await expect(
    page
      .locator('li[data-tone]')
      .filter({ hasText: 'Removed build-cache. Updating the index in the background.' }),
  ).toBeVisible()
  await expect(
    page.getByRole('button', {
      name: 'Remove exclusion for /Users/alex/Projects/web/node_modules',
    }),
  ).toBeFocused()
})

test('search index unavailable offers a retry', async ({ page }) => {
  await gotoScenario(page, screenshotScenarios.searchIndexUnavailable.light)
  await openSettingsSection(page, 'searchIndex')
  await expect(page.getByRole('button', { name: 'Try again' })).toBeVisible()
})
