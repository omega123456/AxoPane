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

  // The app derives keyboard hints and the Windows-only native shell-extension
  // menu section from `navigator.platform`. Pin it per scenario so platform
  // dependent UI is deterministic regardless of the host OS the run happens on.
  if (scenario.platform) {
    await page.addInitScript((platform) => {
      Object.defineProperty(navigator, 'platform', {
        configurable: true,
        get: () => (platform === 'macos' ? 'MacIntel' : 'Win32'),
      })
    }, scenario.platform)
  }
}

export async function gotoScenario(page: Page, scenario: PlaywrightScenario) {
  await injectScenario(page, scenario)
  await page.goto('/')
}

export async function openSettingsSection(
  page: Page,
  section: 'keybindings' | 'columns' | 'layout' | 'updates' | 'logs',
) {
  await page.getByRole('button', { name: 'Settings' }).click()
  if (section === 'keybindings') {
    await page.getByRole('button', { name: 'Keybindings' }).click()
  } else if (section === 'columns') {
    await page.getByRole('button', { name: 'Columns' }).click()
  } else if (section === 'layout') {
    await page.getByRole('button', { name: 'View & Layout' }).click()
  } else if (section === 'updates') {
    await page.getByRole('button', { name: 'Updates' }).click()
  } else if (section === 'logs') {
    await page.getByRole('button', { name: 'Logs' }).click()
  }
}

export async function rightClickPane(page: Page, paneName: 'Left pane' | 'Right pane') {
  await page.getByRole('region', { name: paneName }).click({ button: 'right' })
}
