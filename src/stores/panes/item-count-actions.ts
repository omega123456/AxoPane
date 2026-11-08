import {
  requestVisibleItemCounts as requestVisibleItemCountsCommand,
  sortActiveItems,
} from '@/lib/ipc/commands'
import type { ItemCountEvent } from '@/lib/types/ipc'
import { panesStore } from '@/stores/panes/store-handle'
import {
  appliedItemCounts,
  canCountVisibleItems,
  canStartItemsSort,
  completedItemsSort,
  failedItemsSort,
  itemCountContext,
  startedItemsSort,
  supersededItemsSort,
  visibleItemCountPaths,
  withoutRequestedItemCounts,
  withRequestedItemCounts,
  type ItemCountScope,
  type ItemsSortOperation,
  type ItemsSortScope,
} from '@/stores/panes/item-counts'
import type { PaneState, PanesData } from '@/stores/panes/pane-state'
import { activeTab } from '@/stores/tabs-store'
import type { PaneId } from '@/types/pane'

function itemCountScope(paneId: PaneId, pane: PaneState): ItemCountScope {
  const activeTabId = activeTab(paneId).id
  return { paneId, activeTabId, context: itemCountContext(paneId, pane, activeTabId) }
}

function itemsSortScope(paneId: PaneId, pane: PaneState): ItemsSortScope {
  return {
    ...itemCountScope(paneId, pane),
    expected: { filterApplied: pane.filterApplied, sortDirection: pane.sortDirection },
  }
}

function withLiveActiveTab<Scope extends ItemCountScope>(scope: Scope): Scope {
  return { ...scope, activeTabId: activeTab(scope.paneId).id }
}

function activeTabIds() {
  return { left: activeTab('left').id, right: activeTab('right').id }
}

function setPanes(patch: Partial<PanesData> | null) {
  if (patch) {
    panesStore().setState(patch)
  }
}

async function runActiveItemsSort(scope: ItemsSortScope) {
  const pane = panesStore().getState().panes[scope.paneId]
  if (!canStartItemsSort(pane, scope)) {
    return
  }

  const started = startedItemsSort(panesStore().getState(), withLiveActiveTab(scope))
  if (!started) {
    return
  }
  panesStore().setState(started.next)
  const operation: ItemsSortOperation = { ...scope, operationId: started.operationId }

  try {
    const response = await sortActiveItems({
      context: scope.context,
      sortDirection: scope.expected.sortDirection,
      filter: scope.expected.filterApplied,
      showHidden: panesStore().getState().showHiddenFiles,
    })
    const liveOperation = withLiveActiveTab(operation)
    if (response.kind === 'superseded') {
      setPanes(supersededItemsSort(panesStore().getState(), liveOperation, response.context))
      return
    }
    setPanes(completedItemsSort(panesStore().getState(), liveOperation, response))
  } catch (error) {
    setPanes(failedItemsSort(panesStore().getState(), withLiveActiveTab(operation), error))
  }
}

export function runActiveItemsSortForPane(paneId: PaneId) {
  const pane = panesStore().getState().panes[paneId]
  return runActiveItemsSort(itemsSortScope(paneId, pane))
}

export async function requestVisibleItemCounts(
  paneId: PaneId,
  visibleStartIndex: number,
  visibleEndIndex: number,
  viewportCount: number,
) {
  const pane = panesStore().getState().panes[paneId]
  if (!canCountVisibleItems(pane)) {
    return
  }

  const scope = itemCountScope(paneId, pane)
  const countWindow = { visibleStartIndex, visibleEndIndex, viewportCount }
  const pending = visibleItemCountPaths(
    pane,
    scope.context,
    countWindow,
    panesStore().getState().passiveItemCountRequests,
  )
  if (pending.length === 0) {
    return
  }

  setPanes(withRequestedItemCounts(panesStore().getState(), withLiveActiveTab(scope), pending))
  try {
    await requestVisibleItemCountsCommand({ context: scope.context, paths: pending })
  } catch (error) {
    panesStore().setState((state) => withoutRequestedItemCounts(state, scope.context, pending))
    throw error
  }
}

export function applyItemCountEvents(events: ItemCountEvent[]) {
  setPanes(appliedItemCounts(panesStore().getState(), events, activeTabIds()))
}
