import { expect, test } from '@playwright/test'
import { gotoScenario } from './helpers'
import { screenshotScenarios } from '../src/tests/playwright-fixtures/e2e'

/** Matches the badge's `translate-x-4` / `translate-y-5`, which the zoom scales too. */
const BADGE_OFFSET = { x: 16, y: 20 }

for (const [zoom, scenario] of [
  ['150', screenshotScenarios.nativeDragZoomed.light],
  ['80', screenshotScenarios.nativeDragZoomed.dark],
] as const) {
  test(`keeps the drag badge on the pointer at ${zoom}% app zoom`, async ({ page }) => {
    await gotoScenario(page, scenario)

    const pane = page.getByLabel('Left pane')
    const source = pane.getByRole('row', { name: /Report\.txt/ }).first()
    const folder = pane.getByRole('row', { name: /Documents/ }).first()
    await expect(folder).toBeVisible()
    await source.dispatchEvent('dragstart')

    // Drive the OS cursor to the middle of the folder row, in the same viewport
    // pixels a real drag would report.
    const box = (await folder.boundingBox()) ?? { x: 0, y: 0, width: 0, height: 0 }
    const pointer = { x: box.x + box.width / 2, y: box.y + box.height / 2 }
    await page.evaluate(
      ([x, y]) => {
        const ratio = window.devicePixelRatio || 1
        window.__PLAYWRIGHT_IPC_EMIT__?.('drag://position', {
          cursorX: x * ratio,
          cursorY: y * ratio,
          frameWidth: window.innerWidth * ratio,
          frameHeight: window.innerHeight * ratio,
          ctrlKey: false,
          shiftKey: false,
          altKey: false,
          metaKey: false,
        })
      },
      [pointer.x, pointer.y],
    )

    const badge = page.getByText('Move', { exact: true })
    await expect(badge).toBeVisible()

    // The badge sits at the pointer plus its own offset, which scales with zoom.
    // Without cancelling the zoom the badge lands at pointer × factor — hundreds
    // of pixels away at 150%.
    const factor = Number(zoom) / 100
    const badgeBox = (await badge.boundingBox()) ?? { x: 0, y: 0, width: 0, height: 0 }
    expect(Math.abs(badgeBox.x - (pointer.x + BADGE_OFFSET.x * factor))).toBeLessThan(2)
    expect(Math.abs(badgeBox.y - (pointer.y + BADGE_OFFSET.y * factor))).toBeLessThan(2)
  })
}
