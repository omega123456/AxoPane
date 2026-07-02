import { expect, test } from '@playwright/test'
import { gotoScenario, openSettingsSection, rightClickPane } from './helpers'
import { screenshotScenarios } from '../src/tests/playwright-fixtures/e2e'
import { RELATIVE_DATES_NOW } from '../src/tests/playwright-fixtures/relative-dates'
import { everythingAvailable, everythingUnavailable } from '../src/tests/playwright-fixtures/states'
import {
  deletingQueueFinalProgressEvent,
  expandedQueueFinalProgressEvent,
} from '../src/tests/playwright-fixtures/queue'

for (const mode of ['light', 'dark'] as const) {
  test(`browsing ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.browsing[mode])
    await expect(page.getByRole('region', { name: 'Left pane' })).toBeVisible()
    await expect(page.getByRole('row', { name: /Documents/ }).first()).toBeVisible()
    await expect(page.locator('main')).toHaveScreenshot(`dual-pane-browsing-${mode}.png`)
  })

  test(`marquee selection ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.browsing[mode])
    await expect(page.getByRole('region', { name: 'Left pane' })).toBeVisible()
    await expect(page.getByRole('row', { name: /Documents/ }).first()).toBeVisible()

    const scroller = page.getByTestId('file-pane-scroll-left')
    const box = await scroller.boundingBox()
    if (!box) {
      throw new Error('missing scroll container bounding box')
    }

    // Start below the (short) fixture list, in genuinely empty background, and
    // drag up over the rows — starting a mousedown on a row selects just that
    // row instead, matching Explorer.
    await page.mouse.move(box.x + 20, box.y + 150)
    await page.mouse.down()
    await page.mouse.move(box.x + 240, box.y + 10, { steps: 10 })
    await expect(page.getByTestId('marquee-selection')).toBeVisible()
    await expect(page.locator('main')).toHaveScreenshot(`marquee-selection-${mode}.png`)
    await page.mouse.up()
  })

  test(`sticky tree ${mode}`, async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 520 })
    await gotoScenario(page, screenshotScenarios.stickyTree[mode])
    const treeScroll = page.getByTestId('folder-tree-scroll')
    await expect(treeScroll).toBeVisible()
    await expect(treeScroll.getByRole('button', { name: 'Designs', exact: true })).toBeVisible()
    await treeScroll.evaluate((element) => {
      element.scrollTop = 420
    })
    await expect(page.locator('aside').first()).toHaveScreenshot(`sticky-tree-items-${mode}.png`)
  })

  test(`tree context menu dual pane ${mode}`, async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 520 })
    await gotoScenario(page, screenshotScenarios.stickyTree[mode])
    const treeScroll = page.getByTestId('folder-tree-scroll')
    const designsRow = treeScroll.getByRole('button', { name: 'Designs', exact: true })
    await expect(designsRow).toBeVisible()

    await designsRow.click({ button: 'right' })
    const menu = page.getByRole('menu', { name: 'Designs' })
    await expect(menu).toBeVisible()
    await expect(menu.getByRole('menuitem', { name: 'Open in right pane' })).toBeVisible()
    await expect(menu.getByRole('menuitem', { name: 'Open in left pane' })).toBeVisible()
    await expect(page.locator('main')).toHaveScreenshot(`tree-context-menu-dual-pane-${mode}.png`)
  })

  test(`tabs ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.tabs[mode])
    const leftPane = page.getByRole('region', { name: 'Left pane' })
    await expect(leftPane).toBeVisible()
    await expect(leftPane.getByRole('tab')).toHaveCount(4)
    await expect(page.locator('main')).toHaveScreenshot(`pane-tabs-${mode}.png`)
  })

  test(`file types ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.fileTypes[mode])
    await expect(page.getByRole('region', { name: 'Left pane' })).toBeVisible()
    await expect(page.getByRole('row', { name: /installer\.exe/ }).first()).toBeVisible()
    await expect(page.locator('main')).toHaveScreenshot(`mixed-file-types-${mode}.png`)
  })

  test(`relative dates ${mode}`, async ({ page }) => {
    // Pin the wall clock so each entry's age — and therefore its colour-coded
    // relative phrase — is deterministic regardless of when the run happens.
    await page.clock.setFixedTime(new Date(RELATIVE_DATES_NOW))
    await gotoScenario(page, screenshotScenarios.relativeDates[mode])
    await expect(page.getByText('15 minutes ago').first()).toBeVisible()
    await expect(page.getByText('1 day ago').first()).toBeVisible()
    await expect(page.locator('main')).toHaveScreenshot(`relative-dates-${mode}.png`)
  })

  test(`loading ${mode}`, async ({ page }) => {
    await page.clock.install()
    await gotoScenario(page, screenshotScenarios.loading[mode])
    await page.clock.fastForward(1_001)
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
    await expect(
      page.getByText('You do not have permission to view this folder.').first(),
    ).toBeVisible()
    await expect(page.locator('main')).toHaveScreenshot(`permission-denied-state-${mode}.png`)
  })

  test(`trash ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.trash[mode])
    await expect(page.getByRole('region', { name: 'Left pane' })).toBeVisible()
    await expect(page.getByRole('row', { name: /report\.txt/ }).first()).toBeVisible()
    await expect(page.locator('main')).toHaveScreenshot(`trash-browsing-${mode}.png`)
  })

  test(`size states ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.sizes[mode])
    await expect(page.getByText('941.9 MB').first()).toBeVisible()
    await expect(page.getByRole('row', { name: /Documents/ }).first()).toBeVisible()
    await expect(page.locator('main')).toHaveScreenshot(`folder-size-states-${mode}.png`)
  })

  test(`queue collapsed ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.queueCollapsed[mode])
    await expect(page.getByRole('button', { name: 'Expand job queue' })).toBeVisible()
    await expect(page.locator('main')).toHaveScreenshot(`transfer-queue-collapsed-${mode}.png`)
  })

  test(`queue expanded ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.queueExpanded[mode])
    await page.getByRole('button', { name: 'Expand job queue' }).click()
    await expect(page.getByRole('region', { name: 'Job queue' })).toBeVisible()
    await expect(
      page.getByRole('progressbar', {
        name: `Copying ${expandedQueueFinalProgressEvent.totalItems.toLocaleString()} items`,
      }),
    ).toHaveAttribute(
      'aria-valuenow',
      String(Math.round(expandedQueueFinalProgressEvent.progressPercent)),
    )
    // The chart is throttled, so wait until its filled extent reaches the final
    // seeded percent before capturing (condition-based, no fixed delay).
    await expect(
      page.locator('[data-testid="throughput-chart-fill-extent"]').first(),
    ).toHaveAttribute('width', String(expandedQueueFinalProgressEvent.progressPercent))
    await expect(page.locator('main')).toHaveScreenshot(`transfer-queue-expanded-${mode}.png`)
  })

  test(`queue deleting collapsed ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.queueDeleting[mode])
    await expect(page.getByText('Deleting 1 job')).toBeVisible()
    await expect(page.locator('main')).toHaveScreenshot(
      `transfer-queue-deleting-collapsed-${mode}.png`,
    )
  })

  test(`queue deleting expanded ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.queueDeleting[mode])
    await page.getByRole('button', { name: 'Expand job queue' }).click()
    await expect(page.getByRole('region', { name: 'Job queue' })).toBeVisible()
    await expect(page.getByRole('progressbar', { name: 'Deleting 84 items' })).toHaveAttribute(
      'aria-valuenow',
      String(Math.round(deletingQueueFinalProgressEvent.progressPercent)),
    )
    await expect(
      page.locator('[data-testid="throughput-chart-fill-extent"]').first(),
    ).toHaveAttribute('width', String(deletingQueueFinalProgressEvent.progressPercent))
    await expect(page.locator('main')).toHaveScreenshot(
      `transfer-queue-deleting-expanded-${mode}.png`,
    )
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
    // Pinned to macOS so the Windows+Everything-only "Automatically calculate
    // folder sizes" row never renders here, keeping this baseline stable
    // regardless of the host OS the suite runs on.
    await gotoScenario(page, { ...screenshotScenarios.browsing[mode], platform: 'macos' })
    await openSettingsSection(page, 'layout')
    await expect(page.getByText('Default pane mode')).toBeVisible()
    await expect(page.locator('main')).toHaveScreenshot(`settings-layout-${mode}.png`)
  })

  test(`settings layout with auto folder size toggle ${mode}`, async ({ page }) => {
    // Pinned to Windows with Everything available so the auto folder size
    // toggle renders deterministically regardless of the host OS.
    await gotoScenario(page, {
      ...screenshotScenarios.browsing[mode],
      platform: 'windows',
      commands: {
        ...screenshotScenarios.browsing[mode].commands,
        everything_status: everythingAvailable,
      },
    })
    await openSettingsSection(page, 'layout')
    await expect(
      page.getByRole('switch', { name: 'Automatically calculate folder sizes' }),
    ).toBeVisible()
    await expect(page.locator('main')).toHaveScreenshot(
      `settings-layout-auto-folder-size-${mode}.png`,
    )
  })

  test(`settings dates ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.browsing[mode])
    await openSettingsSection(page, 'dates')
    await expect(page.getByRole('combobox', { name: 'Date format' })).toBeVisible()
    await expect(page.locator('main')).toHaveScreenshot(`settings-dates-${mode}.png`)
  })

  test(`settings updates ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.browsing[mode])
    await openSettingsSection(page, 'updates')
    await expect(page.getByLabel('Update check frequency')).toBeVisible()
    await expect(page.locator('main')).toHaveScreenshot(`settings-updates-${mode}.png`)
  })

  test(`settings logs ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.browsing[mode])
    await openSettingsSection(page, 'logs')
    await expect(page.getByLabel('Capture level')).toBeVisible()
    await expect(
      page.getByText('copy failed: permission denied (E:\\backup\\report.pdf)'),
    ).toBeVisible()
    await expect(page.locator('main')).toHaveScreenshot(`settings-logs-${mode}.png`)
  })

  test(`pane context menu ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.paneContextMenu[mode])
    await rightClickPane(page, 'Left pane')
    await expect(page.getByRole('menu', { name: 'This folder' })).toBeVisible()
    await expect(page.locator('main')).toHaveScreenshot(`pane-context-menu-${mode}.png`)
  })

  test(`row context menu ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.rowContextMenu[mode])
    await page
      .getByRole('row', { name: /Documents/ })
      .first()
      .click({ button: 'right' })
    const menu = page.getByRole('menu', { name: 'Documents' })
    await expect(menu).toBeVisible()
    await expect(menu.getByRole('menuitem', { name: 'Open in VS Code' })).toBeVisible()
    await expect(page.locator('main')).toHaveScreenshot(`row-context-menu-${mode}.png`)
  })

  test(`row context menu submenu ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.rowContextMenu[mode])
    await page
      .getByRole('row', { name: /Documents/ })
      .first()
      .click({ button: 'right' })
    const menu = page.getByRole('menu', { name: 'Documents' })
    await expect(menu).toBeVisible()
    const submenuParent = menu.getByRole('menuitem', { name: '7-Zip' })
    await expect(submenuParent).toBeVisible()
    await submenuParent.hover()
    const submenu = page.getByRole('menu', { name: '7-Zip' })
    await expect(submenu).toBeVisible()
    const [menuBox, submenuParentBox, submenuBox] = await Promise.all([
      menu.boundingBox(),
      submenuParent.boundingBox(),
      submenu.boundingBox(),
    ])
    expect(menuBox).not.toBeNull()
    expect(submenuParentBox).not.toBeNull()
    expect(submenuBox).not.toBeNull()
    expect(submenuBox!.x).toBeGreaterThanOrEqual(8)
    expect(submenuBox!.x + submenuBox!.width).toBeLessThanOrEqual(page.viewportSize()!.width - 8)
    expect(Math.abs(submenuBox!.y - submenuParentBox!.y)).toBeLessThanOrEqual(12)
    await expect(page.locator('main')).toHaveScreenshot(`row-context-menu-submenu-${mode}.png`)
  })

  test(`row context menu submenu left fallback ${mode}`, async ({ page }) => {
    await page.setViewportSize({ width: 520, height: 720 })
    await gotoScenario(page, screenshotScenarios.rowContextMenu[mode])
    const row = page.getByRole('row', { name: /Documents/ }).first()
    const rowBox = await row.boundingBox()
    expect(rowBox).not.toBeNull()
    await page.mouse.click((rowBox?.x ?? 0) + (rowBox?.width ?? 0) - 4, (rowBox?.y ?? 0) + 12, {
      button: 'right',
    })

    const menu = page.getByRole('menu', { name: 'Documents' })
    await expect(menu).toBeVisible()
    await menu.getByRole('menuitem', { name: '7-Zip' }).hover({ force: true })
    const submenu = page.getByRole('menu', { name: '7-Zip' })
    await expect(submenu).toBeVisible()
    const [menuBox, submenuBox] = await Promise.all([menu.boundingBox(), submenu.boundingBox()])
    expect(menuBox).not.toBeNull()
    expect(submenuBox).not.toBeNull()
    expect(submenuBox!.x + submenuBox!.width).toBeLessThanOrEqual(page.viewportSize()!.width - 8)
    expect(submenuBox!.x).toBeGreaterThanOrEqual(8)
    expect(submenuBox!.x + submenuBox!.width).toBeLessThanOrEqual(menuBox!.x + menuBox!.width)
  })

  test(`row context menu loading ${mode}`, async ({ page }) => {
    await page.clock.install()
    await gotoScenario(page, screenshotScenarios.rowContextMenuLoading[mode])
    await page
      .getByRole('row', { name: /Documents/ })
      .first()
      .click({ button: 'right' })
    await expect(page.getByRole('menu', { name: 'Documents' })).toBeVisible()
    await page.clock.fastForward(1_001)
    await expect(page.getByRole('status', { name: 'Loading native menu items' })).toBeVisible()
    await expect(page.locator('main')).toHaveScreenshot(`row-context-menu-loading-${mode}.png`)
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
    await page
      .getByRole('row', { name: /Documents/ })
      .first()
      .click({ button: 'right' })
    const menu = page.getByRole('menu', { name: 'Documents' })
    await expect(menu).toBeVisible()
    await menu
      .getByRole('group', { name: 'Quick actions' })
      .getByRole('menuitem', { name: 'Rename' })
      .click()
    await expect(page.getByRole('textbox', { name: /Rename Documents/ })).toBeVisible()
    await expect(page.locator('main')).toHaveScreenshot(`rename-inline-editor-${mode}.png`)
  })

  test(`delete dialog ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.browsing[mode])
    await page
      .getByRole('row', { name: /Documents/ })
      .first()
      .click({ button: 'right' })
    const menu = page.getByRole('menu', { name: 'Documents' })
    await expect(menu).toBeVisible()
    await menu.getByRole('menuitem', { name: 'Delete permanently' }).click()
    await expect(page.getByRole('dialog', { name: 'Confirm delete' })).toBeVisible()
    await expect(page.locator('main')).toHaveScreenshot(`delete-confirmation-dialog-${mode}.png`)
  })

  test(`calculate all sizes button ${mode}`, async ({ page }) => {
    // Everything unavailable is one of the two conditions that surfaces the
    // toolbar button (the other is auto folder size being off).
    await gotoScenario(page, {
      ...screenshotScenarios.browsing[mode],
      commands: {
        ...screenshotScenarios.browsing[mode].commands,
        everything_status: everythingUnavailable,
      },
    })
    await expect(
      page.getByRole('button', { name: 'Calculate all folder sizes in Left pane' }),
    ).toBeVisible()
    await expect(page.locator('main')).toHaveScreenshot(`calculate-all-sizes-button-${mode}.png`)
  })

  test(`calculate all sizes confirmation dialog ${mode}`, async ({ page }) => {
    await gotoScenario(page, {
      ...screenshotScenarios.browsing[mode],
      commands: {
        ...screenshotScenarios.browsing[mode].commands,
        everything_status: everythingUnavailable,
      },
    })
    await page.getByRole('button', { name: 'Calculate all folder sizes in Left pane' }).click()
    await expect(
      page.getByRole('dialog', { name: 'Confirm calculate all folder sizes' }),
    ).toBeVisible()
    await expect(page.locator('main')).toHaveScreenshot(
      `calculate-all-sizes-confirmation-dialog-${mode}.png`,
    )
  })

  test(`archive dialog ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.browsing[mode])
    await page
      .getByRole('row', { name: /Documents/ })
      .first()
      .click({ button: 'right' })
    const menu = page.getByRole('menu', { name: 'Documents' })
    await expect(menu).toBeVisible()
    await menu.getByRole('menuitem', { name: 'Compress' }).click()
    await expect(page.getByRole('dialog', { name: 'Confirm compress' })).toBeVisible()
    await expect(page.locator('main')).toHaveScreenshot(`archive-confirmation-dialog-${mode}.png`)
  })

  test(`extract dialog ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.fileTypes[mode])
    await page
      .getByRole('row', { name: /bundle\.zip/ })
      .first()
      .click({ button: 'right' })
    const menu = page.getByRole('menu', { name: 'bundle.zip' })
    await expect(menu).toBeVisible()
    await menu.getByRole('menuitem', { name: 'Extract' }).click()
    await expect(page.getByRole('dialog', { name: 'Confirm extract' })).toBeVisible()
    await expect(page.locator('main')).toHaveScreenshot(`extract-confirmation-dialog-${mode}.png`)
  })

  test(`properties dialog ${mode}`, async ({ page }) => {
    // `defaultAppDialog` pins `platform: 'macos'` regardless of the host OS
    // actually running Playwright, so the macOS-only "Default App" row and
    // "Set Default Application…" button render deterministically here even
    // when this suite runs on Windows CI.
    await gotoScenario(page, screenshotScenarios.defaultAppDialog[mode])
    await page
      .getByRole('row', { name: /report\.pdf/ })
      .first()
      .click({ button: 'right' })
    const menu = page.getByRole('menu', { name: 'report.pdf' })
    await expect(menu).toBeVisible()
    await menu.getByRole('menuitem', { name: 'Properties' }).click()
    await expect(page.getByRole('dialog', { name: 'Properties' })).toBeVisible()
    await expect(page.getByText('Default App', { exact: true })).toBeVisible()
    await expect(page.getByText('Fixture Preview')).toBeVisible()
    await expect(page.getByRole('button', { name: 'Set Default Application…' })).toBeVisible()
    await expect(page.locator('main')).toHaveScreenshot(`properties-dialog-${mode}.png`)
  })

  test(`set default application dialog ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.defaultAppDialog[mode])
    await page
      .getByRole('row', { name: /report\.pdf/ })
      .first()
      .click({ button: 'right' })
    const menu = page.getByRole('menu', { name: 'report.pdf' })
    await expect(menu).toBeVisible()
    await menu.getByRole('menuitem', { name: 'Properties' }).click()
    await expect(page.getByRole('dialog', { name: 'Properties' })).toBeVisible()
    await expect(page.getByText('Default App', { exact: true })).toBeVisible()
    await expect(page.getByText('Fixture Preview')).toBeVisible()
    await page.getByRole('button', { name: 'Set Default Application…' }).click()
    await expect(page.getByRole('dialog', { name: 'Set Default Application' })).toBeVisible()
    await expect(page.getByRole('option', { name: 'Fixture Preview' })).toBeVisible()
    await expect(page.locator('main')).toHaveScreenshot(
      `set-default-application-dialog-${mode}.png`,
    )
  })

  test(`set default application error ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.defaultAppDialogError[mode])
    await page
      .getByRole('row', { name: /report\.pdf/ })
      .first()
      .click({ button: 'right' })
    const menu = page.getByRole('menu', { name: 'report.pdf' })
    await expect(menu).toBeVisible()
    await menu.getByRole('menuitem', { name: 'Properties' }).click()
    await expect(page.getByRole('dialog', { name: 'Properties' })).toBeVisible()
    await page.getByRole('button', { name: 'Set Default Application…' }).click()
    await expect(page.getByRole('dialog', { name: 'Set Default Application' })).toBeVisible()
    await page.getByRole('option', { name: 'Fixture Preview' }).click()
    await page.getByRole('button', { name: 'Change All…' }).click()
    await expect(page.getByRole('alert')).toBeVisible()
    await expect(page.locator('main')).toHaveScreenshot(`set-default-application-error-${mode}.png`)
  })
}
