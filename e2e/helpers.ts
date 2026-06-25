import type { Page } from '@playwright/test'
import type { PlaywrightScenario } from '../src/tests/playwright-fixtures/e2e'

async function injectScenario(page: Page, scenario: PlaywrightScenario) {
  await page.addInitScript((value) => {
    ;(
      window as unknown as {
        __PLAYWRIGHT_IPC_SCENARIO__?: PlaywrightScenario
      }
    ).__PLAYWRIGHT_IPC_SCENARIO__ = value
  }, scenario)
}

export async function gotoScenario(page: Page, scenario: PlaywrightScenario) {
  await injectScenario(page, scenario)
  await page.goto('/')
}

export async function openSettingsSection(
  page: Page,
  section: 'keybindings' | 'columns' | 'layout',
) {
  await page.getByRole('button', { name: 'Settings' }).click()
  if (section !== 'keybindings') {
    await page.getByRole('button', { name: new RegExp(`^${section}$`, 'i') }).click()
  }
}

export async function rightClickPane(page: Page, paneName: 'Left pane' | 'Right pane') {
  await page.getByRole('region', { name: paneName }).click({ button: 'right' })
}
