import { expect, type Locator, type Page } from '@playwright/test'
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

const settingsSectionButtons = {
  keybindings: { name: 'Keybindings' },
  columns: { name: 'Columns' },
  layout: { name: 'View and layout' },
  searchIndex: { name: 'Search index', exact: true },
  dates: { name: 'Dates', exact: true },
  updates: { name: 'Updates' },
  logs: { name: 'Logs' },
}

export async function openSettingsSection(
  page: Page,
  section: keyof typeof settingsSectionButtons,
) {
  await page.getByRole('button', { name: 'Settings' }).click()
  await page.getByRole('button', settingsSectionButtons[section]).click()
}

export async function rightClickPane(page: Page, paneName: 'Left pane' | 'Right pane') {
  await page.getByRole('region', { name: paneName }).click({ button: 'right' })
}

/** The visible card inside a modal's full-window overlay. */
export function dialogPanel(dialog: Locator) {
  return dialog.locator(':scope > div')
}

export const cutShortcut = process.platform === 'darwin' ? 'Meta+x' : 'Control+x'

export async function openRowContextMenu(page: Page, rowName: string) {
  await page
    .getByRole('row', { name: new RegExp(escapeRegExp(rowName)) })
    .first()
    .click({ button: 'right' })
  const menu = page.getByRole('menu', { name: rowName })
  await expect(menu).toBeVisible()
  return menu
}

function escapeRegExp(text: string) {
  return text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

const CLIP_PADDING_PX = 8

type Box = { x: number; y: number; width: number; height: number }

async function requireBox(locator: Locator): Promise<Box> {
  const box = await locator.boundingBox()
  if (!box) throw new Error('missing element geometry')
  return box
}

/** The page area covering every locator, so detached popups share one screenshot. */
export async function unionClip(page: Page, locators: Locator[]): Promise<Box> {
  const boxes = await Promise.all(locators.map(requireBox))
  const viewport = page.viewportSize()
  if (!viewport) throw new Error('missing viewport size')
  const left = Math.max(0, Math.min(...boxes.map((box) => box.x)) - CLIP_PADDING_PX)
  const top = Math.max(0, Math.min(...boxes.map((box) => box.y)) - CLIP_PADDING_PX)
  const right = Math.min(
    viewport.width,
    Math.max(...boxes.map((box) => box.x + box.width)) + CLIP_PADDING_PX,
  )
  const bottom = Math.min(
    viewport.height,
    Math.max(...boxes.map((box) => box.y + box.height)) + CLIP_PADDING_PX,
  )
  return { x: left, y: top, width: right - left, height: bottom - top }
}

const DRAG_STEP_COUNT = 6

type Point = { x: number; y: number }

async function centerOf(locator: Locator): Promise<Point> {
  const box = await locator.boundingBox()
  if (!box) throw new Error('missing element geometry')
  return { x: box.x + box.width / 2, y: box.y + box.height / 2 }
}

export async function pressAndDragInWholePixelSteps(page: Page, source: Locator, target: Locator) {
  const start = await centerOf(source)
  const end = await centerOf(target)
  await page.mouse.move(start.x, start.y)
  await page.mouse.down()
  for (let step = 1; step <= DRAG_STEP_COUNT; step++) {
    const progress = step / DRAG_STEP_COUNT
    const x = start.x + Math.round((end.x - start.x) * progress)
    const y = start.y + Math.round((end.y - start.y) * progress)
    await page.mouse.move(x, y)
  }
}
