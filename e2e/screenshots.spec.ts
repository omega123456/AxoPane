import { expect, test } from '@playwright/test'
import {
  cutShortcut,
  dialogPanel,
  gotoScenario,
  openRowContextMenu,
  openSettingsSection,
  pressAndDragInWholePixelSteps,
  rightClickPane,
  unionClip,
} from './helpers'
import { screenshotScenarios } from '../src/tests/playwright-fixtures/e2e'
import { RELATIVE_DATES_NOW } from '../src/tests/playwright-fixtures/relative-dates'
import { everythingAvailable } from '../src/tests/playwright-fixtures/states'
import { searchIndexTypedInsideExisting } from '../src/tests/playwright-fixtures/search-index'
import { expandedQueueFinalProgressEvent } from '../src/tests/playwright-fixtures/queue'

for (const mode of ['light', 'dark'] as const) {
  test(`full app ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.fullApp[mode])
    const leftPane = page.getByRole('region', { name: 'Left pane' })
    await leftPane
      .getByRole('row', { name: /Documents/ })
      .first()
      .click()
    await page.getByRole('region', { name: 'Right pane' }).click()
    await expect(page.getByTestId('active-pane-rule')).toHaveCount(1)
    await expect(page.getByRole('status', { name: 'Everything unavailable' })).toBeVisible()
    await expect(page.getByRole('button', { name: 'Expand job queue' })).toBeVisible()
    await expect(page.locator('main')).toHaveScreenshot(`full-app-${mode}.png`)
  })

  test(`command bar ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.fullApp[mode])
    const toolbar = page.getByRole('toolbar', { name: 'Command bar' })
    await expect(toolbar.getByRole('button', { name: 'Back in Left pane' })).toBeVisible()
    await expect(toolbar.getByRole('button', { name: 'Up in Left pane' })).toBeVisible()
    await expect(
      toolbar.getByRole('button', { name: 'Create new folder in Left pane' }),
    ).toBeVisible()
    await expect(
      toolbar.getByRole('button', { name: 'Create new file in Left pane' }),
    ).toBeVisible()
    await expect(
      toolbar.getByRole('button', { name: 'Calculate all folder sizes in Left pane' }),
    ).toBeVisible()
    await expect(toolbar.getByRole('textbox', { name: 'Left pane filter' })).toBeVisible()
    const refresh = toolbar.getByRole('button', { name: 'Refresh Left pane' })
    await refresh.focus()
    await expect(refresh).toBeFocused()
    await expect(toolbar).toHaveScreenshot(`command-bar-${mode}.png`)
  })

  test(`view menu ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.browsing[mode])
    await page.getByLabel('View: Details').first().click()
    const menu = page.getByRole('menu', { name: 'View options' })
    const details = menu.getByRole('menuitemradio', { name: 'Details' })
    await expect(details).toHaveAttribute('aria-checked', 'true')
    await expect(details).toBeFocused()
    await expect(menu).toHaveScreenshot(`pane-view-menu-${mode}.png`)
  })

  test(`icons view ${mode}`, async ({ page }) => {
    await page.setViewportSize({ width: 720, height: 700 })
    await gotoScenario(page, screenshotScenarios.iconsInteractionStates[mode])
    const leftPane = page.getByRole('region', { name: 'Left pane' })
    const grid = page.getByRole('grid', { name: /Icons for/ })
    await expect(grid).toHaveAttribute('aria-colcount', '1')
    const longName = grid.getByRole('gridcell', { name: /A very long photograph filename/ })
    await longName.click()
    await page.keyboard.press(cutShortcut)
    await expect(grid.getByRole('gridcell', { name: 'Hidden reference.txt' })).toBeVisible()
    await grid.getByRole('gridcell', { name: 'Mountain preview.png' }).click()
    await page.keyboard.press('F2')
    await expect(page.getByLabel('Rename Mountain preview.png')).toBeVisible()
    await expect(leftPane).toHaveScreenshot(`pane-icons-${mode}.png`)
  })

  test(`thumbnails view ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.thumbnailsFallback[mode])
    const leftPane = page.getByRole('region', { name: 'Left pane' })
    const grid = page.getByRole('grid', { name: /Large thumbnails for/ })
    await expect(grid.getByRole('img').first()).toBeVisible()
    await expect(grid.getByRole('gridcell', { name: 'Unsupported artwork.psd' })).toBeVisible()
    const preview = grid.getByRole('gridcell', { name: 'Mountain preview.png' })
    await preview.click()
    await page.keyboard.press(cutShortcut)
    await grid.getByRole('gridcell', { name: 'Unreadable scan.tiff' }).click()
    await page.keyboard.press('F2')
    await expect(page.getByLabel('Rename Unreadable scan.tiff')).toBeVisible()
    await expect(leftPane).toHaveScreenshot(`pane-thumbnails-${mode}.png`)
  })

  test(`marquee selection ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.browsing[mode])
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
    await expect(page.getByRole('region', { name: 'Left pane' })).toHaveScreenshot(
      `marquee-selection-${mode}.png`,
    )
    await page.mouse.up()
  })

  test(`folder tree ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.folderTree[mode])
    const sidebar = page.locator('aside').first()
    const treeScroll = page.getByTestId('folder-tree-scroll')
    await expect(treeScroll.getByRole('button', { name: 'Designs', exact: true })).toBeVisible()
    await expect(treeScroll.getByRole('button', { name: '3 other folders' })).toBeVisible()
    await expect(treeScroll.locator('[data-capacity-level="warning"]')).toBeVisible()
    await expect(treeScroll.locator('[data-capacity-level="critical"]')).toBeVisible()
    await expect(page.getByTestId('favourites-panel').getByText('Drop folders here')).toBeVisible()
    const firstRow = treeScroll.getByRole('treeitem').first().getByRole('button').last()
    await firstRow.focus()
    await expect(firstRow).toBeFocused()
    await expect(sidebar).toHaveScreenshot(`folder-tree-${mode}.png`)
  })

  test(`sticky tree ${mode}`, async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 520 })
    await gotoScenario(page, screenshotScenarios.stickyTree[mode])
    const treeScroll = page.getByTestId('folder-tree-scroll')
    await expect(treeScroll).toBeVisible()
    await expect(treeScroll.getByRole('button', { name: 'Designs', exact: true })).toBeVisible()
    // Reveal every folded level so the tree is long enough to scroll the
    // (capped) pinned band into place.
    const foldRows = treeScroll.getByRole('button', { name: /other folders?$/ })
    // Dispatch the clicks so the pointer never rests on the tree; a real click
    // leaves a hover tint on whichever row the reveal shifts under it.
    while ((await foldRows.count()) > 0) {
      await foldRows.first().dispatchEvent('click')
    }
    await treeScroll.evaluate((element) => {
      element.scrollTop = 420
    })
    await expect(page.locator('aside').first()).toHaveScreenshot(`sticky-tree-items-${mode}.png`)
  })

  test(`favourites ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.favouritesPanel[mode])
    const panel = page.getByTestId('favourites-panel')
    await expect(panel.locator('[data-favourite-row]')).toHaveCount(9)
    await expect(panel.getByRole('button', { name: 'Code', exact: true })).toBeVisible()
    await panel.getByRole('button', { name: 'Downloads', exact: true }).click({ button: 'right' })
    await page
      .getByRole('menu', { name: 'Downloads' })
      .getByRole('menuitem', { name: 'Rename Favourite' })
      .click()
    await expect(panel.getByRole('textbox', { name: 'Rename favourite Downloads' })).toBeFocused()
    const handle = page.getByRole('separator', { name: 'Resize favourites' })
    // A zero-height divider never counts as visible to hover(); point at it
    // directly, where its grab zone overhangs the line.
    const box = await handle.boundingBox()
    if (!box) throw new Error('favourites resize handle has no box')
    await page.mouse.move(box.x + box.width / 2, box.y)
    await expect(page.locator('aside').first()).toHaveScreenshot(`favourites-${mode}.png`)
  })

  test(`tree context menu ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.ejectMenu[mode])
    await page
      .getByTestId('folder-tree-scroll')
      .getByRole('button', { name: 'USB Stick (E:)', exact: true })
      .click({ button: 'right' })
    const menu = page.getByRole('menu', { name: 'USB Stick (E:)' })
    await expect(menu.getByRole('menuitem', { name: 'Open in right pane' })).toBeVisible()
    await expect(menu.getByRole('menuitem', { name: 'Eject' })).toBeVisible()
    await expect(menu).toHaveScreenshot(`tree-context-menu-${mode}.png`)
  })

  test(`error infobar ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.ejectRefused[mode])
    await page
      .getByTestId('folder-tree-scroll')
      .getByRole('button', { name: 'USB Stick (E:)', exact: true })
      .click({ button: 'right' })
    await page
      .getByRole('menu', { name: 'USB Stick (E:)' })
      .getByRole('menuitem', { name: 'Eject' })
      .click()
    const infobar = page.getByRole('alert').filter({ hasText: /is in use/ })
    await expect(infobar).toBeVisible()
    await expect(infobar).toHaveScreenshot(`error-infobar-${mode}.png`)
  })

  test(`tabs ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.tabs[mode])
    const leftTabs = page.getByRole('group', { name: 'Left pane tabs' })
    await expect(leftTabs.getByRole('tab')).toHaveCount(4)
    await expect(page.getByLabel(/Locked tab/)).toBeVisible()
    await expect(leftTabs).toHaveScreenshot(`pane-tabs-${mode}.png`)
  })

  test(`tab drag ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.tabDrag[mode])
    const leftTabs = page.getByRole('group', { name: 'Left pane tabs' })
    const tabs = leftTabs.getByRole('tab')
    await pressAndDragInWholePixelSteps(page, tabs.nth(0), tabs.nth(2))
    await expect(leftTabs).toHaveScreenshot(`tab-drag-${mode}.png`)
    await page.mouse.up()
    await expect(leftTabs.getByRole('tab').last()).toHaveText('Omega')
  })

  test(`breadcrumbs ${mode}`, async ({ page }) => {
    await page.setViewportSize({ width: 720, height: 520 })
    await gotoScenario(page, screenshotScenarios.breadcrumbs[mode])
    const rightPane = page.getByRole('region', { name: 'Right pane' })
    await expect(rightPane.getByTestId('breadcrumb-collapse-marker')).toBeVisible()
    await expect(rightPane.getByRole('button', { name: 'Foxtrot' })).toBeVisible()
    await expect(rightPane).toHaveScreenshot(`breadcrumb-overflow-${mode}.png`)
  })

  test(`details view ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.fileTypes[mode])
    const leftPane = page.getByRole('region', { name: 'Left pane' })
    await expect(leftPane.getByRole('row', { name: /installer\.exe/ }).first()).toBeVisible()
    await leftPane
      .getByRole('row', { name: /notes\.txt/ })
      .first()
      .click()
    await page.keyboard.press('F2')
    await expect(page.getByRole('textbox', { name: /Rename notes\.txt/ })).toBeVisible()
    await expect(leftPane).toHaveScreenshot(`details-view-${mode}.png`)
  })

  test(`relative dates ${mode}`, async ({ page }) => {
    // Pin the wall clock so each entry's age — and therefore its colour-coded
    // relative phrase — is deterministic regardless of when the run happens.
    await page.clock.setFixedTime(new Date(RELATIVE_DATES_NOW))
    await gotoScenario(page, screenshotScenarios.relativeDates[mode])
    await expect(page.getByText('15 minutes ago').first()).toBeVisible()
    await expect(page.getByText('1 day ago').first()).toBeVisible()
    await expect(page.getByText('on Friday').first()).toBeVisible()
    await expect(page.getByRole('region', { name: 'Left pane' })).toHaveScreenshot(
      `relative-dates-${mode}.png`,
    )
  })

  test(`folder sizes ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.sizeBounds[mode])
    const lowerBound = page.getByText('≥ 941.9 MB').first()
    await expect(lowerBound).toHaveAttribute(
      'title',
      "Doesn't include Desktop, Downloads, Mail and 2 more. Open them to include them.",
    )
    await expect(page.getByText('N/A').first()).toHaveAttribute(
      'title',
      'macOS blocked access. Grant Full Disk Access to include this folder.',
    )
    const statusBar = page.locator('footer').first()
    await expect(statusBar.getByText('Updating folder sizes')).toBeAttached()
    await expect(page.getByRole('region', { name: 'Left pane' })).toHaveScreenshot(
      `folder-size-bounds-${mode}.png`,
    )
    await expect(statusBar).toHaveScreenshot(`status-bar-size-spinner-${mode}.png`)
  })

  test(`permission denied ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.permissionWindows[mode])
    const leftPane = page.getByRole('region', { name: 'Left pane' })
    await expect(leftPane.getByText("You don't have access")).toBeVisible()
    await expect(leftPane.getByRole('button', { name: 'Open as administrator' })).toBeVisible()
    await expect(leftPane).toHaveScreenshot(`permission-denied-state-${mode}.png`)
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
    await page.getByRole('button', { name: 'Show items' }).first().click()
    await expect(page.getByRole('button', { name: 'Hide items' }).first()).toHaveAttribute(
      'aria-expanded',
      'true',
    )
    // The chart is throttled, so wait until its filled extent reaches the final
    // seeded percent before capturing (condition-based, no fixed delay).
    await expect(
      page.locator('[data-testid="throughput-chart-fill-extent"]').first(),
    ).toHaveAttribute('width', String(expandedQueueFinalProgressEvent.progressPercent))
    await expect(page.locator('[data-testid="throughput-chart"]').first()).toHaveAttribute(
      'data-scale-settled',
      'true',
    )
    await expect(page.locator('main')).toHaveScreenshot(`transfer-queue-expanded-${mode}.png`)
  })

  test(`conflict dialog ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.conflict[mode])
    const dialog = page.getByRole('dialog', { name: 'Resolve file conflict' })
    await expect(dialog).toBeVisible()
    await expect(dialogPanel(dialog)).toHaveScreenshot(`file-conflict-dialog-${mode}.png`)
  })

  test(`settings keybindings ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.browsing[mode])
    await openSettingsSection(page, 'keybindings')
    await expect(page.getByLabel('Search keybindings')).toBeVisible()
    await expect(page.getByRole('dialog', { name: 'Settings' })).toHaveScreenshot(
      `settings-keybindings-${mode}.png`,
    )
  })

  test(`settings columns ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.browsing[mode])
    await openSettingsSection(page, 'columns')
    await expect(page.getByRole('switch', { name: 'Created column' })).toBeVisible()
    await expect(page.getByRole('dialog', { name: 'Settings' })).toHaveScreenshot(
      `settings-columns-${mode}.png`,
    )
  })

  test(`settings layout ${mode}`, async ({ page }) => {
    // Pinned to Windows with Everything available so the auto folder size
    // toggle renders deterministically regardless of the host OS.
    await gotoScenario(page, {
      ...screenshotScenarios.browsing[mode],
      platform: 'windows',
      commands: {
        ...screenshotScenarios.browsing[mode].commands,
        folder_size_index_status: everythingAvailable,
      },
    })
    await openSettingsSection(page, 'layout')
    await expect(page.getByRole('radiogroup', { name: 'Default view for new tabs' })).toBeVisible()
    const autoFolderSize = page.getByRole('switch', {
      name: 'Automatically calculate folder sizes',
    })
    await autoFolderSize.scrollIntoViewIfNeeded()
    await expect(autoFolderSize).toBeInViewport()
    await expect(page.getByRole('dialog', { name: 'Settings' })).toHaveScreenshot(
      `settings-layout-${mode}.png`,
    )
  })

  test(`settings search index ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.searchIndexValidation[mode])
    await openSettingsSection(page, 'searchIndex')
    const field = page.getByRole('textbox', { name: 'Folder to exclude' })
    await field.fill(searchIndexTypedInsideExisting)
    await field.press('Enter')
    await expect(
      page.locator('li[data-tone]').filter({ hasText: /Already inside node_modules/ }),
    ).toBeVisible()
    await expect(page.getByRole('dialog', { name: 'Settings' })).toHaveScreenshot(
      `settings-search-index-${mode}.png`,
    )
  })

  test(`settings dates ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.browsing[mode])
    await openSettingsSection(page, 'dates')
    await expect(page.getByRole('combobox', { name: 'Date format' })).toBeVisible()
    const expander = page.getByRole('button', { name: /Reset to defaults/ })
    await expander.click()
    await expect(expander).toHaveAttribute('aria-expanded', 'true')
    await expect(page.getByRole('button', { name: 'Reset', exact: true })).toBeVisible()
    await expect(page.getByRole('dialog', { name: 'Settings' })).toHaveScreenshot(
      `settings-dates-${mode}.png`,
    )
  })

  test(`settings updates ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.browsing[mode])
    await openSettingsSection(page, 'updates')
    await expect(page.getByLabel('Update check frequency')).toBeVisible()
    await expect(page.getByRole('dialog', { name: 'Settings' })).toHaveScreenshot(
      `settings-updates-${mode}.png`,
    )
  })

  test(`settings logs ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.browsing[mode])
    await openSettingsSection(page, 'logs')
    await expect(page.getByLabel('Capture level')).toBeVisible()
    await expect(
      page.getByText('copy failed: permission denied (E:\\backup\\report.pdf)'),
    ).toBeVisible()
    await expect(page.getByRole('dialog', { name: 'Settings' })).toHaveScreenshot(
      `settings-logs-${mode}.png`,
    )
  })

  test(`search everywhere ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.searchNameResults[mode])
    await page.getByRole('button', { name: 'Search everywhere' }).click()
    const palette = page.getByRole('dialog', { name: 'Search everywhere' })
    await palette.getByRole('combobox', { name: 'Search everywhere' }).fill('report')
    await expect(palette.getByText('Q3 report.docx', { exact: true }).first()).toBeVisible()
    await expect(palette).toHaveScreenshot(`search-everywhere-${mode}.png`)
  })

  test(`row context menu ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.rowContextMenu[mode])
    const menu = await openRowContextMenu(page, 'Documents')
    await expect(menu.getByRole('menuitem', { name: 'Open in VS Code' })).toBeVisible()
    const submenuParent = menu.getByRole('menuitem', { name: '7-Zip' })
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
    await expect(page).toHaveScreenshot(`row-context-menu-${mode}.png`, {
      clip: await unionClip(page, [menu, submenu]),
    })
  })

  test(`new folder dialog ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.browsing[mode])
    await rightClickPane(page, 'Left pane')
    await page.getByRole('menuitem', { name: 'New folder' }).click()
    const dialog = page.getByRole('dialog', { name: 'New folder' })
    await expect(dialog).toBeVisible()
    await expect(dialogPanel(dialog)).toHaveScreenshot(`new-folder-dialog-${mode}.png`)
  })

  test(`administrator permissions dialog ${mode}`, async ({ page }) => {
    // Windows-only prompt: pin the platform so the run stays deterministic on
    // a macOS host, where an access-denied error has a different answer.
    await gotoScenario(page, {
      ...screenshotScenarios.browsing[mode],
      platform: 'windows',
      commandErrors: {
        create_folder: 'Failed to create folder: Access is denied. (os error 5)',
      },
    })
    await rightClickPane(page, 'Left pane')
    await page.getByRole('menuitem', { name: 'New folder' }).click()
    await page.getByLabel('Folder name').fill('Reports')
    await page
      .getByRole('dialog', { name: 'New folder' })
      .getByRole('button', { name: 'Create', exact: true })
      .click()
    const dialog = page.getByRole('dialog', { name: 'Administrator permissions needed' })
    await expect(dialog).toBeVisible()
    await expect(dialogPanel(dialog)).toHaveScreenshot(
      `administrator-permissions-dialog-${mode}.png`,
    )
  })

  test(`confirmation dialog ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.browsing[mode])
    const menu = await openRowContextMenu(page, 'Documents')
    await menu.getByRole('menuitem', { name: 'Delete permanently' }).click()
    const dialog = page.getByRole('dialog', { name: 'Confirm delete' })
    await expect(dialog).toBeVisible()
    await expect(dialogPanel(dialog)).toHaveScreenshot(`delete-confirmation-dialog-${mode}.png`)
  })

  test(`properties dialog ${mode}`, async ({ page }) => {
    // `defaultAppDialog` pins `platform: 'macos'` regardless of the host OS
    // actually running Playwright, so the macOS-only "Default App" row and
    // "Set Default Application…" button render deterministically here even
    // when this suite runs on Windows CI.
    await gotoScenario(page, screenshotScenarios.defaultAppDialog[mode])
    const menu = await openRowContextMenu(page, 'report.pdf')
    await menu.getByRole('menuitem', { name: 'Properties' }).click()
    const dialog = page.getByRole('dialog', { name: 'Properties' })
    await expect(dialog.getByText('Default App', { exact: true })).toBeVisible()
    await expect(dialog.getByText('Fixture Preview')).toBeVisible()
    await expect(dialog.getByRole('button', { name: 'Set Default Application…' })).toBeVisible()
    await expect(dialogPanel(dialog)).toHaveScreenshot(`properties-dialog-${mode}.png`)
  })

  test(`set default application dialog ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.defaultAppDialogError[mode])
    const menu = await openRowContextMenu(page, 'report.pdf')
    await menu.getByRole('menuitem', { name: 'Properties' }).click()
    await page.getByRole('button', { name: 'Set Default Application…' }).click()
    const dialog = page.getByRole('dialog', { name: 'Set Default Application' })
    await dialog.getByRole('option', { name: 'Fixture Preview' }).click()
    await dialog.getByRole('button', { name: 'Change All…' }).click()
    await expect(dialog.getByRole('alert')).toBeVisible()
    await expect(dialogPanel(dialog)).toHaveScreenshot(`set-default-application-dialog-${mode}.png`)
  })

  test(`native drag badge ${mode}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios.nativeDrag[mode])

    const pane = page.getByLabel('Left pane')
    const source = pane.getByRole('row', { name: /Report\.txt/ }).first()
    const folder = pane.getByRole('row', { name: /Documents/ }).first()
    await expect(folder).toBeVisible()

    // Start the gesture from the DOM: this arms the bridge and calls
    // `start_native_drag`, which the scenario holds open for the screenshot.
    await source.dispatchEvent('dragstart')

    // Then place the OS cursor over the folder, as Rust's stream would. The
    // payload is physical pixels relative to the window frame, with the frame
    // sized to the viewport so there is no chrome to subtract.
    const box = (await folder.boundingBox()) ?? { x: 0, y: 0, width: 0, height: 0 }
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
      [box.x + box.width / 2, box.y + box.height / 2],
    )

    // Same volume and no modifier, so this is a move — deliberately not asserting
    // a modifier-driven copy, whose key differs per platform.
    await expect(page.getByText('Move', { exact: true })).toBeVisible()
    await expect(page.getByRole('region', { name: 'Left pane' })).toHaveScreenshot(
      `native-drag-badge-${mode}.png`,
    )
  })
}
