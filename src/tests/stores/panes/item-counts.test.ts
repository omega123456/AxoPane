import { describe, expect, it } from 'vitest'
import {
  AUTO_VISIBLE_ITEM_COUNT_LIMIT,
  appliedItemCounts,
  canCountVisibleItems,
  canStartItemsSort,
  clearPassiveItemCountRequestsForPane,
  completedItemsSort,
  failedItemsSort,
  itemCountContext,
  startedItemsSort,
  supersededItemsSort,
  visibleItemCountPaths,
  withoutPatchedItemCountRequests,
  withoutRequestedItemCounts,
  withRequestedItemCounts,
  type ItemsSortScope,
} from '@/stores/panes/item-counts'
import type { PaneState } from '@/stores/panes/pane-state'
import { TRASH_PATH } from '@/lib/trash'
import { directoryEntry, fileEntry, panesDataWithLeft, paneWith } from './pane-fixtures'

const TAB_ID = 'left-tab-1'
const LISTED: Partial<PaneState> = {
  path: '/root',
  listRequestId: 7,
  sortKey: 'items',
  sortDirection: 'desc',
  entries: [directoryEntry('/root/a'), directoryEntry('/root/b')],
}

function sortScope(pane: PaneState): ItemsSortScope {
  return {
    paneId: 'left',
    activeTabId: TAB_ID,
    context: itemCountContext('left', pane, TAB_ID),
    expected: { filterApplied: pane.filterApplied, sortDirection: pane.sortDirection },
  }
}

describe('item counts', () => {
  it('builds a context that is cleared per pane', () => {
    const pane = paneWith('left', LISTED)
    const context = itemCountContext('left', pane, TAB_ID)
    expect(context).toEqual({ paneId: 'left', tabId: TAB_ID, requestId: 7, path: '/root' })
    const requests = { [`left::${TAB_ID}::7::/x`]: true as const, 'right::t::1::/y': true as const }
    expect(clearPassiveItemCountRequestsForPane(requests, 'left')).toEqual({
      'right::t::1::/y': true,
    })
  })

  it('only counts settled real listings', () => {
    expect(canCountVisibleItems(paneWith('left', { ...LISTED, itemsSortStatus: 'idle' }))).toBe(
      true,
    )
    expect(canCountVisibleItems(paneWith('left', { ...LISTED, loading: true }))).toBe(false)
    expect(canCountVisibleItems(paneWith('left', { ...LISTED, itemsSortStatus: 'counting' }))).toBe(
      false,
    )
    expect(canCountVisibleItems(paneWith('left', { ...LISTED, listRequestId: 0 }))).toBe(false)
    expect(canCountVisibleItems(paneWith('left', { ...LISTED, entries: [] }))).toBe(false)
    expect(canCountVisibleItems(paneWith('left', { ...LISTED, path: TRASH_PATH }))).toBe(false)
  })

  it('picks unrequested visible folders within the budget and tracks requests', () => {
    const entries = [directoryEntry('/root/a'), fileEntry('/root/f'), directoryEntry('/root/b')]
    const pane = paneWith('left', { ...LISTED, entries })
    const context = itemCountContext('left', pane, TAB_ID)
    const countWindow = { visibleStartIndex: 0, visibleEndIndex: 0, viewportCount: 2 }
    expect(visibleItemCountPaths(pane, context, countWindow, {})).toEqual(['/root/a', '/root/b'])

    const state = panesDataWithLeft(pane)
    const requested = withRequestedItemCounts(
      state,
      { paneId: 'left', activeTabId: TAB_ID, context },
      ['/root/a'],
    )
    expect(requested).not.toBeNull()
    const passive = requested?.passiveItemCountRequests ?? {}
    expect(visibleItemCountPaths(pane, context, countWindow, passive)).toEqual(['/root/b'])
    expect(
      withoutRequestedItemCounts({ passiveItemCountRequests: passive }, context, ['/root/a']),
    ).toEqual({
      passiveItemCountRequests: {},
    })
    expect(
      withRequestedItemCounts(state, { paneId: 'left', activeTabId: 'other', context }, []),
    ).toBeNull()
  })

  it('returns nothing for an empty window or exhausted budget', () => {
    const pane = paneWith('left', { ...LISTED, entries: [] })
    const context = itemCountContext('left', pane, TAB_ID)
    const countWindow = { visibleStartIndex: 0, visibleEndIndex: 0, viewportCount: 0 }
    expect(visibleItemCountPaths(pane, context, countWindow, {})).toEqual([])

    const full = paneWith('left', LISTED)
    const exhausted = Object.fromEntries(
      Array.from({ length: AUTO_VISIBLE_ITEM_COUNT_LIMIT }, (_, index) => [
        `left::${TAB_ID}::7::/used${index}`,
        true as const,
      ]),
    )
    expect(visibleItemCountPaths(full, context, countWindow, exhausted)).toEqual([])
  })

  it('applies counts only to the matching listing and keeps unchanged panes', () => {
    const state = panesDataWithLeft(LISTED)
    const context = itemCountContext('left', state.panes.left, TAB_ID)
    const events = [{ context, results: [{ path: '/root/a', itemCount: 3 }], done: true }]
    const tabs = { left: TAB_ID, right: 'right-tab-1' }
    expect(appliedItemCounts(state, events, tabs)?.panes.left.entries[0].itemCount).toBe(3)
    expect(appliedItemCounts(state, events, { ...tabs, left: 'other' })).toBeNull()
    expect(appliedItemCounts(state, [], tabs)).toBeNull()
  })

  it('lets a patched uncounted folder be requested again', () => {
    const context = itemCountContext('left', paneWith('left', LISTED), TAB_ID)
    const key = `left::${TAB_ID}::7::/root/a`
    const state = { passiveItemCountRequests: { [key]: true as const } }
    const patch = (entry: ReturnType<typeof directoryEntry> | null) => ({
      tabId: TAB_ID,
      path: '/root',
      reason: 'watch' as const,
      changed: [{ path: '/root/a', entry }],
      removed: [],
    })
    expect(
      withoutPatchedItemCountRequests(state, context, patch(directoryEntry('/root/a'))),
    ).toEqual({})
    expect(withoutPatchedItemCountRequests(state, context, patch(null))).toBe(
      state.passiveItemCountRequests,
    )
  })

  it('runs an items sort through start, completion, staleness and failure', () => {
    const state = panesDataWithLeft({ ...LISTED, focusedEntryId: '/gone' })
    const scope = sortScope(state.panes.left)
    expect(canStartItemsSort(state.panes.left, scope)).toBe(true)
    expect(canStartItemsSort({ ...state.panes.left, loading: true }, scope)).toBe(false)

    const started = startedItemsSort(state, scope)
    expect(started?.operationId).toBe(1)
    const running = { ...state, ...started?.next }
    expect(running.panes.left.itemsSortStatus).toBe('counting')
    const operation = { ...scope, operationId: 1 }

    const response = { context: scope.context, path: '/ROOT', entries: [directoryEntry('/root/b')] }
    const completed = completedItemsSort(running, operation, { ...response, path: '/root' })
    expect(completed?.panes.left.itemsSortStatus).toBe('complete')
    expect(completed?.panes.left.focusedEntryId).toBe('/root/b')
    expect(completedItemsSort(running, operation, { ...response, path: '/elsewhere' })).toBeNull()

    expect(supersededItemsSort(running, operation, scope.context)?.panes.left.itemsSortStatus).toBe(
      'stale',
    )
    const failed = failedItemsSort(running, operation, new Error('boom'))
    expect(failed?.panes.left.error).toBe('boom')
    expect(failedItemsSort(running, operation, 'opaque')?.panes.left.error).toBe(
      'Failed to sort items',
    )
    expect(failedItemsSort(running, { ...operation, operationId: 9 }, 'late')).toBeNull()
    expect(startedItemsSort(state, { ...scope, activeTabId: 'other' })).toBeNull()
  })
})
