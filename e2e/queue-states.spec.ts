import { expect, test } from '@playwright/test'
import { gotoScenario } from './helpers'
import { screenshotScenarios } from '../src/tests/playwright-fixtures/e2e'
import { deletingQueueFinalProgressEvent } from '../src/tests/playwright-fixtures/queue'

const PENDING_JOB_COUNT = 5
const PENDING_JOBS_VISIBLE_WITHOUT_SCROLLING = 3

test('collapsed queue offers to expand', async ({ page }) => {
  await gotoScenario(page, screenshotScenarios.queueCollapsed.light)
  await expect(page.getByRole('button', { name: 'Expand job queue' })).toBeVisible()
})

test('queue keeps a long path job readable', async ({ page }) => {
  await gotoScenario(page, screenshotScenarios.queueLongPath.light)
  await page.getByRole('button', { name: 'Expand job queue' }).click()
  await expect(page.getByRole('region', { name: 'Job queue' })).toBeVisible()
  await expect(page.getByRole('progressbar', { name: 'Copying 1 items' })).toBeVisible()
})

test('queue shows several pending jobs without scrolling', async ({ page }) => {
  await gotoScenario(page, screenshotScenarios.queueManyPending.light)
  await page.getByRole('button', { name: 'Expand job queue' }).click()
  await expect(page.getByRole('region', { name: 'Job queue' })).toBeVisible()
  const pendingRows = page.locator('[aria-label="Queued jobs"] > div')
  await expect(pendingRows).toHaveCount(PENDING_JOB_COUNT)
  for (let index = 0; index < PENDING_JOBS_VISIBLE_WITHOUT_SCROLLING; index += 1) {
    await expect(pendingRows.nth(index)).toBeInViewport()
  }
})

test('collapsed delete job summarises its progress', async ({ page }) => {
  await gotoScenario(page, screenshotScenarios.queueDeleting.light)
  await expect(page.getByText('Deleting 84 items')).toBeVisible()
})

test('expanded delete job counts items without a throughput chart', async ({ page }) => {
  await gotoScenario(page, screenshotScenarios.queueDeleting.light)
  await page.getByRole('button', { name: 'Expand job queue' }).click()
  await expect(page.getByRole('progressbar', { name: 'Deleting 84 items' })).toHaveAttribute(
    'aria-valuenow',
    String(Math.round(deletingQueueFinalProgressEvent.progressPercent)),
  )
  await expect(page.locator('[data-testid="throughput-chart"]')).toHaveCount(0)
  await expect(
    page.getByText(
      `${deletingQueueFinalProgressEvent.completedItems} of ${deletingQueueFinalProgressEvent.totalItems} items deleted ·`,
      { exact: false },
    ),
  ).toBeVisible()
})
