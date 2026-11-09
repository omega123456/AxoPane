import { expect, test } from '@playwright/test'
import {
  gotoScenario,
  openRowContextMenu,
  pressAndDragInWholePixelSteps,
  rightClickPane,
} from './helpers'
import { screenshotScenarios } from '../src/tests/playwright-fixtures/e2e'
import { longTypeLabel } from '../src/tests/playwright-fixtures/file-types'

const SUBMENU_VIEWPORT_MARGIN_PX = 8
const LOADING_INDICATOR_DELAY_MS = 1_001

test('filter follows the active pane', async ({ page }) => {
  await gotoScenario(page, screenshotScenarios.browsing.light)
  const toolbar = page.getByRole('toolbar', { name: 'Command bar' })
  await toolbar.getByRole('textbox', { name: 'Left pane filter' }).fill('doc')
  await page.getByRole('region', { name: 'Right pane' }).click()
  const rightFilter = toolbar.getByRole('textbox', { name: 'Right pane filter' })
  await expect(rightFilter).toHaveValue('')
  await expect(rightFilter).toHaveAttribute('placeholder', /^Filter /)
})

test('long column value stays inside its column', async ({ page }) => {
  await gotoScenario(page, screenshotScenarios.fileTypes.light)
  const row = page.getByRole('row', { name: /mystery\.qwzzz/ }).first()
  await expect(row).toBeVisible()
  const cell = await row.evaluate((element, label) => {
    const match = [...element.children].find((child) => child.textContent === label)
    return match
      ? {
          clientWidth: match.clientWidth,
          scrollWidth: match.scrollWidth,
          overflowX: getComputedStyle(match).overflowX,
        }
      : null
  }, longTypeLabel)
  expect(cell).not.toBeNull()
  // The value is wider than its fixed-width cell, so the cell — not the inline
  // span inside it — has to be the clipping box.
  expect(cell!.scrollWidth).toBeGreaterThan(cell!.clientWidth)
  expect(cell!.overflowX).toBe('hidden')
})

test('icons view stays one column wide in a narrow pane', async ({ page }) => {
  await page.setViewportSize({ width: 720, height: 700 })
  await gotoScenario(page, screenshotScenarios.iconsNarrow.light)
  const grid = page.getByRole('grid', { name: /Icons for/ })
  await expect(
    grid.getByRole('gridcell', { name: /A very long photograph filename/ }),
  ).toBeVisible()
  await expect(grid).toHaveAttribute('aria-colcount', '1')
})

test('thumbnails show a loading placeholder before previews arrive', async ({ page }) => {
  await gotoScenario(page, screenshotScenarios.thumbnailsInteractionStates.light)
  await expect(page.getByLabel('Thumbnail loading').first()).toBeVisible()
})

test('folder loading state', async ({ page }) => {
  await page.clock.install()
  await gotoScenario(page, screenshotScenarios.loading.light)
  await page.clock.fastForward(LOADING_INDICATOR_DELAY_MS)
  await expect(page.getByRole('status', { name: 'Loading folder' }).first()).toBeVisible()
})

test('empty folder state', async ({ page }) => {
  await gotoScenario(page, screenshotScenarios.empty.light)
  await expect(page.getByText('This folder is empty').first()).toBeVisible()
})

test('directory error state', async ({ page }) => {
  await gotoScenario(page, screenshotScenarios.error.light)
  await expect(page.getByText('Directory refresh failed: device timeout.').first()).toBeVisible()
})

test('permission denied state on macOS offers System Settings', async ({ page }) => {
  await gotoScenario(page, screenshotScenarios.permissionMacos.light)
  await expect(page.getByText("You don't have access").first()).toBeVisible()
  await expect(page.getByRole('button', { name: 'Open System Settings' }).first()).toBeVisible()
})

test('trash listing', async ({ page }) => {
  await gotoScenario(page, screenshotScenarios.trash.light)
  await expect(page.getByRole('row', { name: /report\.txt/ }).first()).toBeVisible()
})

test('folder size states', async ({ page }) => {
  await gotoScenario(page, screenshotScenarios.sizes.light)
  await expect(page.getByText('941.9 MB').first()).toBeVisible()
  await expect(page.getByRole('row', { name: /Documents/ }).first()).toBeVisible()
})

test('items sort pending', async ({ page }) => {
  await gotoScenario(page, screenshotScenarios.itemsSortPending.light)
  await expect(page.getByRole('status', { name: 'Counting items…' })).toBeVisible()
  await expect(page.getByRole('columnheader', { name: /Items/ }).first()).toBeVisible()
})

test('address bar switches to edit mode', async ({ page }) => {
  await page.setViewportSize({ width: 720, height: 520 })
  await gotoScenario(page, screenshotScenarios.breadcrumbs.light)
  const rightPane = page.getByRole('region', { name: 'Right pane' })
  await rightPane.getByRole('button', { name: 'Foxtrot' }).click({ button: 'right' })
  const pathEditor = rightPane.getByRole('textbox', { name: 'Right pane path' })
  await expect(pathEditor).toBeFocused()
})

test('tab drag across panes', async ({ page }) => {
  await gotoScenario(page, screenshotScenarios.tabDrag.light)
  const leftTabs = page.getByRole('group', { name: 'Left pane tabs' })
  const rightTabs = page.getByRole('group', { name: 'Right pane tabs' })
  await pressAndDragInWholePixelSteps(
    page,
    leftTabs.getByRole('tab').nth(1),
    rightTabs.getByRole('tab').last(),
  )
  await page.mouse.up()
  await expect(rightTabs.getByRole('tab')).toHaveCount(3)
})

test('tab drag that would empty a pane is refused', async ({ page }) => {
  await gotoScenario(page, screenshotScenarios.invalidTabDrag.light)
  const leftTabs = page.getByRole('group', { name: 'Left pane tabs' })
  const target = page.getByRole('group', { name: 'Right pane tabs' }).getByRole('tab').first()
  await pressAndDragInWholePixelSteps(page, leftTabs.getByRole('tab'), target)
  await page.mouse.up()
  await expect(leftTabs.getByRole('tab')).toHaveCount(1)
})

test('tab context menu', async ({ page }) => {
  await gotoScenario(page, screenshotScenarios.tabDrag.light)
  const tab = page.getByRole('group', { name: 'Left pane tabs' }).getByRole('tab').nth(1)
  await tab.click({ button: 'right' })
  await expect(page.getByRole('menuitem', { name: 'Move tab right' })).toBeVisible()
  await expect(page.getByRole('menuitem', { name: 'Add to Favourites' })).toBeVisible()
})

test('pane context menu', async ({ page }) => {
  await gotoScenario(page, screenshotScenarios.paneContextMenu.light)
  await rightClickPane(page, 'Left pane')
  await expect(page.getByRole('menu', { name: 'This folder' })).toBeVisible()
})

test('row context menu shows a loading row while native items load', async ({ page }) => {
  await page.clock.install()
  await gotoScenario(page, screenshotScenarios.rowContextMenuLoading.light)
  await openRowContextMenu(page, 'Documents')
  await page.clock.fastForward(LOADING_INDICATOR_DELAY_MS)
  await expect(page.getByRole('status', { name: 'Loading native menu items' })).toBeVisible()
})

test('row context submenu falls back to the left near the viewport edge', async ({ page }) => {
  await page.setViewportSize({ width: 520, height: 720 })
  await gotoScenario(page, screenshotScenarios.rowContextMenu.light)
  const row = page.getByRole('row', { name: /Documents/ }).first()
  const scroller = page.getByTestId('file-pane-scroll-left')
  const [rowBox, scrollerBox] = await Promise.all([row.boundingBox(), scroller.boundingBox()])
  expect(rowBox).not.toBeNull()
  expect(scrollerBox).not.toBeNull()

  // With pane-level horizontal overflow enabled, the row can be wider than
  // the visible pane. Click near the visible trailing edge of the pane's
  // scroll viewport rather than the row's full scroll width so the pointer
  // still lands on the rendered row while keeping the menu close enough to
  // the viewport edge to exercise the submenu left-fallback placement.
  const visibleRightEdge = Math.min(
    (rowBox?.x ?? 0) + (rowBox?.width ?? 0),
    (scrollerBox?.x ?? 0) + (scrollerBox?.width ?? 0),
  )
  await page.mouse.click(visibleRightEdge - 4, (rowBox?.y ?? 0) + 12, { button: 'right' })

  const menu = page.getByRole('menu', { name: 'Documents' })
  await expect(menu).toBeVisible()
  await menu.getByRole('menuitem', { name: '7-Zip' }).hover({ force: true })
  const submenu = page.getByRole('menu', { name: '7-Zip' })
  await expect(submenu).toBeVisible()
  const [menuBox, submenuBox] = await Promise.all([menu.boundingBox(), submenu.boundingBox()])
  expect(menuBox).not.toBeNull()
  expect(submenuBox).not.toBeNull()
  const viewportRightLimit = page.viewportSize()!.width - SUBMENU_VIEWPORT_MARGIN_PX
  expect(submenuBox!.x + submenuBox!.width).toBeLessThanOrEqual(viewportRightLimit)
  expect(submenuBox!.x).toBeGreaterThanOrEqual(SUBMENU_VIEWPORT_MARGIN_PX)
  expect(submenuBox!.x + submenuBox!.width).toBeLessThanOrEqual(menuBox!.x + menuBox!.width)
})

test('compress opens a confirmation dialog', async ({ page }) => {
  await gotoScenario(page, screenshotScenarios.browsing.light)
  const menu = await openRowContextMenu(page, 'Documents')
  await menu.getByRole('menuitem', { name: 'Compress' }).click()
  await expect(page.getByRole('dialog', { name: 'Confirm compress' })).toBeVisible()
})

test('extract opens a confirmation dialog', async ({ page }) => {
  await gotoScenario(page, screenshotScenarios.fileTypes.light)
  const menu = await openRowContextMenu(page, 'bundle.zip')
  await menu.getByRole('menuitem', { name: 'Extract' }).click()
  await expect(page.getByRole('dialog', { name: 'Confirm extract' })).toBeVisible()
})

test('calculate all folder sizes asks for confirmation', async ({ page }) => {
  await gotoScenario(page, screenshotScenarios.fullApp.light)
  await page.getByRole('button', { name: 'Calculate all folder sizes in Left pane' }).click()
  await expect(
    page.getByRole('dialog', { name: 'Confirm calculate all folder sizes' }),
  ).toBeVisible()
})

test('set default application lists the candidate apps', async ({ page }) => {
  await gotoScenario(page, screenshotScenarios.defaultAppDialog.light)
  const menu = await openRowContextMenu(page, 'report.pdf')
  await menu.getByRole('menuitem', { name: 'Properties' }).click()
  await page.getByRole('button', { name: 'Set Default Application…' }).click()
  await expect(page.getByRole('dialog', { name: 'Set Default Application' })).toBeVisible()
  await expect(page.getByRole('option', { name: 'Fixture Preview' })).toBeVisible()
})
