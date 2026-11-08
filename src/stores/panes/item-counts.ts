import type {
  DirectoryEntry,
  DirPatchEvent,
  ItemCountEvent,
  ItemCountRequestContext,
  SortDirection,
} from '@/lib/types/ipc'
import type { PaneId } from '@/types/pane'
import { isTrashPath } from '@/lib/trash'
import { pathKey, pathsMatch, samePathOrWindowsCaseFold } from '@/lib/path-compare'
import { withResolvedIcons } from '@/stores/panes/icon-requests'
import {
  isListingSettled,
  patchPaneEntries,
  retainedFocusId,
  updateEntries,
  withPane,
  type PaneState,
  type PanesData,
  type PanesPatch,
  type PendingRequests,
} from '@/stores/panes/pane-state'

export const AUTO_VISIBLE_ITEM_COUNT_LIMIT = 200
const ITEM_COUNT_KEY_SEPARATOR = '::'
const FAILED_ITEMS_SORT_MESSAGE = 'Failed to sort items'

export type ActiveTabIds = Record<PaneId, string>

export type ItemCountScope = {
  paneId: PaneId
  activeTabId: string
  context: ItemCountRequestContext
}

export type ItemsSortExpectation = {
  filterApplied: string
  sortDirection: SortDirection
}

export type ItemsSortScope = ItemCountScope & { expected: ItemsSortExpectation }

export type ItemsSortOperation = ItemsSortScope & { operationId: number }

export type StartedItemsSort = { next: PanesPatch; operationId: number }

export type ItemsSortResponse = {
  context: ItemCountRequestContext
  path: string
  entries: DirectoryEntry[]
}

export type VisibleItemCountWindow = {
  visibleStartIndex: number
  visibleEndIndex: number
  viewportCount: number
}

type PassiveRequestState = Pick<PanesData, 'passiveItemCountRequests'>

function itemCountContextPrefix(context: ItemCountRequestContext) {
  const contextKey = [context.paneId, context.tabId, context.requestId].join(
    ITEM_COUNT_KEY_SEPARATOR,
  )
  return `${contextKey}${ITEM_COUNT_KEY_SEPARATOR}`
}

function itemCountRequestKey(context: ItemCountRequestContext, path: string) {
  return `${itemCountContextPrefix(context)}${pathKey(path)}`
}

function withoutKeysStartingWith(requests: PendingRequests, prefix: string): PendingRequests {
  return Object.fromEntries(
    Object.entries(requests).filter(([key]) => !key.startsWith(prefix)),
  ) as PendingRequests
}

export function clearPassiveItemCountRequestsForPane(
  passiveItemCountRequests: PendingRequests,
  paneId: PaneId,
) {
  return withoutKeysStartingWith(passiveItemCountRequests, `${paneId}${ITEM_COUNT_KEY_SEPARATOR}`)
}

export function itemCountContext(
  paneId: PaneId,
  pane: PaneState,
  activeTabId: string,
): ItemCountRequestContext {
  return {
    paneId,
    tabId: activeTabId,
    requestId: pane.listRequestId,
    path: pane.path,
  }
}

export function itemCountContextMatches(
  pane: PaneState,
  scope: ItemCountScope,
  context: ItemCountRequestContext = scope.context,
) {
  return (
    context.paneId === scope.paneId &&
    context.tabId === scope.activeTabId &&
    context.requestId === pane.listRequestId &&
    pathsMatch(context.path, pane.path)
  )
}

function itemsSortContextMatches(
  pane: PaneState,
  scope: ItemsSortScope,
  context: ItemCountRequestContext = scope.context,
) {
  return (
    itemCountContextMatches(pane, scope, context) &&
    pane.sortKey === 'items' &&
    pane.filterApplied === scope.expected.filterApplied &&
    pane.sortDirection === scope.expected.sortDirection
  )
}

export function canStartItemsSort(pane: PaneState, scope: ItemsSortScope) {
  return isListingSettled(pane) && itemsSortContextMatches(pane, scope)
}

function isSameItemsSortOperation(
  pane: PaneState,
  operation: ItemsSortOperation,
  context: ItemCountRequestContext = operation.context,
) {
  const sameOperationId = pane.itemsSortOperationId === operation.operationId
  return sameOperationId && itemsSortContextMatches(pane, operation, context)
}

export function startedItemsSort(
  state: PanesPatch,
  scope: ItemsSortScope,
): StartedItemsSort | null {
  const pane = state.panes[scope.paneId]
  if (!itemsSortContextMatches(pane, scope)) {
    return null
  }
  const operationId = pane.itemsSortOperationId + 1
  return {
    operationId,
    next: withPane(state, scope.paneId, {
      itemsSortStatus: 'counting',
      itemsSortOperationId: operationId,
      error: null,
    }),
  }
}

export function supersededItemsSort(
  state: PanesPatch,
  operation: ItemsSortOperation,
  responseContext: ItemCountRequestContext,
): PanesPatch | null {
  const pane = state.panes[operation.paneId]
  if (!isSameItemsSortOperation(pane, operation, responseContext)) {
    return null
  }
  return withPane(state, operation.paneId, { itemsSortStatus: 'stale' })
}

export function completedItemsSort(
  state: Pick<PanesData, 'panes' | 'resolvedIconPaths'>,
  operation: ItemsSortOperation,
  response: ItemsSortResponse,
): PanesPatch | null {
  const pane = state.panes[operation.paneId]
  if (!isSameItemsSortOperation(pane, operation, response.context)) {
    return null
  }
  if (!samePathOrWindowsCaseFold(response.path, pane.path)) {
    return null
  }

  const entries = withResolvedIcons(response.entries, state.resolvedIconPaths)
  return withPane(state, operation.paneId, {
    entries,
    focusedEntryId: retainedFocusId(entries, pane.focusedEntryId),
    itemsSortStatus: 'complete',
  })
}

export function failedItemsSort(
  state: PanesPatch,
  operation: ItemsSortOperation,
  error: unknown,
): PanesPatch | null {
  const pane = state.panes[operation.paneId]
  if (!isSameItemsSortOperation(pane, operation)) {
    return null
  }
  return withPane(state, operation.paneId, {
    itemsSortStatus: 'failed',
    error: error instanceof Error ? error.message : FAILED_ITEMS_SORT_MESSAGE,
  })
}

export function canCountVisibleItems(pane: PaneState) {
  if (pane.loading) {
    return false
  }
  if (isTrashPath(pane.path)) {
    return false
  }
  if (pane.itemsSortStatus === 'counting') {
    return false
  }
  if (pane.listRequestId === 0) {
    return false
  }
  return pane.entries.length > 0
}

function needsVisibleItemCount(entry: DirectoryEntry) {
  return entry.isDir && entry.itemCount == null
}

function firstByPathKey(entries: DirectoryEntry[]) {
  const seen = new Set<string>()
  return entries.filter((entry) => {
    const key = pathKey(entry.path)
    if (seen.has(key)) {
      return false
    }
    seen.add(key)
    return true
  })
}

function overscannedRange(pane: PaneState, countWindow: VisibleItemCountWindow) {
  const overscan = Math.max(0, countWindow.viewportCount)
  return {
    startIndex: Math.max(0, countWindow.visibleStartIndex - overscan),
    endIndex: Math.min(pane.entries.length - 1, countWindow.visibleEndIndex + overscan),
  }
}

function requestedCountForContext(
  passiveItemCountRequests: PendingRequests,
  context: ItemCountRequestContext,
) {
  const contextPrefix = itemCountContextPrefix(context)
  return Object.keys(passiveItemCountRequests).filter((key) => key.startsWith(contextPrefix)).length
}

export function visibleItemCountPaths(
  pane: PaneState,
  context: ItemCountRequestContext,
  countWindow: VisibleItemCountWindow,
  passiveItemCountRequests: PendingRequests,
): string[] {
  const { startIndex, endIndex } = overscannedRange(pane, countWindow)
  if (endIndex < startIndex) {
    return []
  }

  const alreadyRequested = requestedCountForContext(passiveItemCountRequests, context)
  const remaining = Math.max(0, AUTO_VISIBLE_ITEM_COUNT_LIMIT - alreadyRequested)
  const visibleEntries = pane.entries.slice(startIndex, endIndex + 1)
  const uncountedFolders = firstByPathKey(visibleEntries.filter(needsVisibleItemCount))
  const notYetRequested = uncountedFolders.filter(
    (entry) => !passiveItemCountRequests[itemCountRequestKey(context, entry.path)],
  )
  return notYetRequested.slice(0, remaining).map((entry) => entry.path)
}

export function withRequestedItemCounts(
  state: PassiveRequestState & PanesPatch,
  scope: ItemCountScope,
  paths: string[],
): PassiveRequestState | null {
  const pane = state.panes[scope.paneId]
  if (!itemCountContextMatches(pane, scope)) {
    return null
  }

  const requests = withoutKeysStartingWith(
    state.passiveItemCountRequests,
    itemCountContextPrefix(scope.context),
  )
  for (const path of paths) {
    requests[itemCountRequestKey(scope.context, path)] = true
  }
  return { passiveItemCountRequests: requests }
}

export function withoutRequestedItemCounts(
  state: PassiveRequestState,
  context: ItemCountRequestContext,
  paths: string[],
): PassiveRequestState {
  const requests = { ...state.passiveItemCountRequests }
  for (const path of paths) {
    delete requests[itemCountRequestKey(context, path)]
  }
  return { passiveItemCountRequests: requests }
}

function isDirectoryAtPath(entry: DirectoryEntry, path: string) {
  return entry.isDir && pathsMatch(entry.path, path)
}

function entriesWithItemCounts(
  pane: PaneState,
  scope: Omit<ItemCountScope, 'context'>,
  events: ItemCountEvent[],
) {
  const results = events
    .filter((event) => itemCountContextMatches(pane, { ...scope, context: event.context }))
    .flatMap((event) => event.results)
  return updateEntries(
    pane.entries,
    results,
    (result) => pane.entries.findIndex((entry) => isDirectoryAtPath(entry, result.path)),
    (entry, result) =>
      entry.itemCount === result.itemCount ? null : { ...entry, itemCount: result.itemCount },
  )
}

export function appliedItemCounts(
  state: PanesPatch,
  events: ItemCountEvent[],
  activeTabIds: ActiveTabIds,
): PanesPatch | null {
  if (events.length === 0) {
    return null
  }
  const panes = patchPaneEntries(state.panes, (paneId, pane) =>
    entriesWithItemCounts(pane, { paneId, activeTabId: activeTabIds[paneId] }, events),
  )
  return panes ? { panes } : null
}

function invalidatesItemCount(change: DirPatchEvent['changed'][number]) {
  const entry = change.entry
  if (!entry?.isDir) {
    return false
  }
  return entry.itemCount == null
}

export function withoutPatchedItemCountRequests(
  state: PassiveRequestState,
  context: ItemCountRequestContext,
  event: DirPatchEvent,
) {
  const invalidatedKeys = new Set(
    event.changed
      .filter(invalidatesItemCount)
      .map((change) => itemCountRequestKey(context, change.path)),
  )
  if (invalidatedKeys.size === 0) {
    return state.passiveItemCountRequests
  }
  return Object.fromEntries(
    Object.entries(state.passiveItemCountRequests).filter(([key]) => !invalidatedKeys.has(key)),
  ) as PendingRequests
}
