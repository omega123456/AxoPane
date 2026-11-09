import { expect, test } from '@playwright/test'
import { gotoScenario } from './helpers'
import { screenshotScenarios } from '../src/tests/playwright-fixtures/e2e'

const searchStates = [
  { name: 'recents', scenario: 'searchRecents', query: '', expectText: 'Desktop' },
  { name: 'browse', scenario: 'searchBrowse', query: 'alex/', expectText: 'invoice.pdf' },
  {
    name: 'no results',
    scenario: 'searchNoResults',
    query: 'taxes 2019',
    expectText: 'No results for “taxes 2019”',
  },
  {
    name: 'Everything not running',
    scenario: 'searchEverythingNotRunning',
    query: 'report',
    expectText: 'Everything isn’t running',
  },
  {
    name: 'macOS indexing',
    scenario: 'searchMacIndexing',
    query: 'photo',
    expectText: 'Photo Booth Library',
  },
  {
    name: 'macOS complete',
    scenario: 'searchMacComplete',
    query: 'photo',
    expectText: 'Files and folders on this Mac · 6 results',
  },
  {
    name: 'macOS updating',
    scenario: 'searchMacUpdating',
    query: 'photo',
    expectText: 'Updating index… · 6 results',
  },
  {
    name: 'macOS paused',
    scenario: 'searchMacPaused',
    query: 'photo',
    expectText: 'Results may be incomplete until indexing resumes.',
  },
  {
    name: 'macOS indexing with no results',
    scenario: 'searchMacIndexingNoResults',
    query: 'taxes 2019',
    expectText: 'No results yet for “taxes 2019”',
  },
  {
    name: 'macOS folders-only fallback with no results',
    scenario: 'searchMacFallbackNoResults',
    query: 'taxes 2019',
    expectText: 'No folders match “taxes 2019”',
  },
  {
    name: 'macOS unavailable',
    scenario: 'searchMacUnavailable',
    query: 'photo',
    expectText: 'Search is unavailable',
  },
  {
    name: 'loading more',
    scenario: 'searchLoadingMore',
    query: 'invoice',
    expectText: 'Loading more…',
  },
] as const

for (const state of searchStates) {
  test(`search everywhere ${state.name}`, async ({ page }) => {
    await gotoScenario(page, screenshotScenarios[state.scenario].light)
    await page.getByRole('button', { name: 'Search everywhere' }).click()
    const palette = page.getByRole('dialog', { name: 'Search everywhere' })
    await palette.getByRole('combobox', { name: 'Search everywhere' }).fill(state.query)
    await expect(palette.getByText(state.expectText, { exact: true }).first()).toBeVisible()
  })
}
