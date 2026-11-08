import { describe, expect, it } from 'vitest'
import {
  DEFAULT_TAB_VIEW,
  directoryListingResolved,
  findRevealEntry,
  focusRequested,
  initializedPanes,
  listingFailed,
  listingStarted,
  listingViewOf,
  matchesListingView,
  navigatedPanes,
  paneResetForTab,
  revealTargetAfterNavigation,
  tabViewOf,
  trashListingResolved,
  withAppliedFilter,
  withClearedFilter,
  withFilterDraft,
  withFilterTimer,
  withFocusedEntry,
  withHistoryIndex,
  withScrollPosition,
} from '@/stores/panes/listing-transitions'
import { directoryEntry, fileEntry, panesData, panesDataWithLeft } from './pane-fixtures'

const NO_SELECTION = { focusedId: null, selectedIds: [] }

describe('listing transitions', () => {
  it('initializes both panes from their active tabs and keeps a newer index status', () => {
    const newerStatus = { isAvailable: true } as never
    const next = initializedPanes(panesData({ folderSizeIndexStatus: newerStatus }), {
      activePaneId: 'right',
      showHiddenFiles: true,
      folderSizeIndexStatus: { isAvailable: false } as never,
      volumes: [],
      leftTab: { path: '/l', ...DEFAULT_TAB_VIEW },
      rightTab: { path: '/r', sortKey: 'size', sortDirection: 'desc', filter: 'x' },
      treeRoots: ['/'],
      treeNodes: {},
    })
    expect(next.folderSizeIndexStatus).toBe(newerStatus)
    expect(next.panes?.right).toMatchObject({ path: '/r', filterApplied: 'x', history: ['/r'] })
    expect(next.panes?.left.historyIndex).toBe(0)
  })

  it('compares listing views and maps panes to tab views', () => {
    const state = panesDataWithLeft({ path: '/root', filterApplied: 'f' })
    const view = listingViewOf(state.panes.left)
    expect(matchesListingView(state.panes.left, view)).toBe(true)
    expect(matchesListingView({ ...state.panes.left, sortKey: 'size' }, view)).toBe(false)
    expect(tabViewOf(state.panes.left)).toEqual({ ...DEFAULT_TAB_VIEW, path: '/root', filter: 'f' })
  })

  it('reveals the folder navigated up from unless an explicit target is given', () => {
    expect(revealTargetAfterNavigation('/root/child', '/root', undefined)).toBe('/root/child')
    expect(revealTargetAfterNavigation('/root', '/other', undefined)).toBeUndefined()
    expect(revealTargetAfterNavigation('/root', '/other', '/other/hit')).toBe('/other/hit')
    const entries = [fileEntry('/root/a')]
    expect(findRevealEntry(entries, '/root/a')).toBe(entries[0])
    expect(findRevealEntry(entries, undefined)).toBeUndefined()
  })

  it('starts, resolves and fails a listing', () => {
    const started = listingStarted(panesDataWithLeft({ listingGeneration: 2 }), 'left')
    expect(started.panes.left).toMatchObject({
      loading: true,
      listingGeneration: 3,
      listRequestId: 0,
    })

    const state = panesDataWithLeft(
      { focusedEntryId: '/root/b', historyIndex: -1 },
      { passiveItemCountRequests: { 'left::t::1::/x': true } },
    )
    const sortedEntries = [directoryEntry('/root/a'), directoryEntry('/root/b')]
    const listing = {
      paneId: 'left' as const,
      path: '/root',
      sortedEntries,
      selection: NO_SELECTION,
    }
    const resolved = directoryListingResolved(state, {
      ...listing,
      requestId: 5,
      revealEntry: undefined,
      revealScrollRequestId: 0,
    })
    expect(resolved.panes?.left).toMatchObject({
      focusedEntryId: '/root/b',
      listRequestId: 5,
      history: ['/root'],
      listingComplete: true,
    })
    expect(resolved.passiveItemCountRequests).toEqual({})
    expect(resolved.revealScrollRequests).toBe(state.revealScrollRequests)

    const revealed = directoryListingResolved(state, {
      ...listing,
      requestId: 6,
      revealEntry: sortedEntries[0],
      revealScrollRequestId: 3,
    })
    expect(revealed.panes?.left.focusedEntryId).toBe('/root/a')
    expect(revealed.revealScrollRequests).toEqual({ left: 3 })

    const trash = trashListingResolved(
      panesDataWithLeft({ historyIndex: 1, history: ['/a', '/b'] }),
      {
        ...listing,
        selection: { focusedId: 'missing', selectedIds: ['/root/b'] },
      },
    )
    expect(trash.panes.left).toMatchObject({ focusedEntryId: '/root/b', historyIndex: 1 })

    expect(listingFailed(state, 'left', new Error('nope')).panes.left.error).toBe('nope')
    expect(listingFailed(state, 'left', 42).panes.left.error).toBe('Failed to load directory')
  })

  it('pushes history and resets the filter on a fresh navigation only', () => {
    const state = panesDataWithLeft({
      path: '/a',
      history: ['/a', '/b'],
      historyIndex: 0,
      filterApplied: 'x',
      scrollPositions: { '/c': 10, '/a': 5 },
    })
    const fresh = navigatedPanes(state, 'left', '/c', false)
    expect(fresh.panes?.left).toMatchObject({
      path: '/c',
      history: ['/a', '/c'],
      historyIndex: 1,
      filterApplied: '',
      scrollPositions: { '/a': 5 },
    })
    expect(fresh.filterTimers).toEqual({ left: undefined })
    expect(navigatedPanes(state, 'left', '/a', false).panes?.left.history).toEqual(['/a'])

    const viaHistory = navigatedPanes(state, 'left', '/b', true)
    expect(viaHistory.panes?.left).toMatchObject({ path: '/b', filterApplied: 'x' })
    expect(viaHistory.filterTimers).toBeUndefined()
    expect(withHistoryIndex(state, 'left', 1).panes.left.historyIndex).toBe(1)
  })

  it('resets a pane for a newly active tab and requests focus', () => {
    const state = panesDataWithLeft(
      { entries: [fileEntry('/x')], history: ['/x'], historyIndex: 0 },
      { passiveItemCountRequests: { 'left::t::1::/x': true }, focusRequestId: 4 },
    )
    const reset = paneResetForTab(state, 'left', { path: '/tab', ...DEFAULT_TAB_VIEW, filter: 'q' })
    expect(reset.panes.left).toMatchObject({
      path: '/tab',
      entries: [],
      filterApplied: 'q',
      historyIndex: -1,
    })
    expect(reset.passiveItemCountRequests).toEqual({})
    expect(focusRequested(state, 'right')).toEqual({
      activePaneId: 'right',
      focusRequestId: 5,
      focusRequestPaneId: 'right',
    })
  })

  it('drafts, applies and clears filters, recounting items sorts', () => {
    const state = panesDataWithLeft({ sortKey: 'items', itemsSortStatus: 'complete' })
    expect(withFilterDraft(state, 'left', 'ab').panes.left).toMatchObject({
      filterDraft: 'ab',
      typing: true,
    })
    expect(withAppliedFilter(state, 'left', 'ab').panes.left).toMatchObject({
      filterApplied: 'ab',
      itemsSortStatus: 'counting',
    })
    const named = panesDataWithLeft({ itemsSortStatus: 'complete', filterDraft: 'x' })
    const cleared = withClearedFilter({ ...named, filterTimers: { left: 3 } }, 'left')
    expect(cleared.panes.left).toMatchObject({ filterDraft: '', itemsSortStatus: 'complete' })
    expect(cleared.filterTimers).toEqual({ left: undefined })
    expect(withFilterTimer(named, 'right', 9).filterTimers).toEqual({ right: 9 })
  })

  it('records focus and scroll positions per pane', () => {
    const state = panesData()
    expect(withFocusedEntry(state, 'left', 'id').panes.left.focusedEntryId).toBe('id')
    expect(withScrollPosition(state, 'left', '/a', 40).panes.left.scrollPositions).toEqual({
      '/a': 40,
    })
  })
})
