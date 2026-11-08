import type { SortDirection, SortKey } from '@/lib/types/ipc'
import type { PaneViewMode } from '@/lib/pane-view'
import type { PaneId } from '@/types/pane'
import { clamp } from '@/lib/numbers'
import { moveItem } from '@/lib/collections'

export type TabState = {
  id: string
  path: string
  sortKey: SortKey
  sortDirection: SortDirection
  filter: string
  viewMode: PaneViewMode
  locked: boolean
}

export type PaneTabs = {
  activeTabIndex: number
  tabs: TabState[]
}

export type TabPanes = Record<PaneId, PaneTabs>

export type TabMoveResult =
  | { kind: 'none' }
  | { kind: 'reorder'; paneId: PaneId; tabId: string }
  | {
      kind: 'transfer'
      sourcePaneId: PaneId
      destinationPaneId: PaneId
      sourceActiveChanged: boolean
      destinationTabId: string
    }

export type TabPatch = Partial<
  Pick<TabState, 'path' | 'sortKey' | 'sortDirection' | 'filter' | 'viewMode'>
>

export type TabMoveRequest = {
  sourcePaneId: PaneId
  tabId: string
  destinationPaneId: PaneId
  destinationIndex: number
}

export type PlannedTabMove = { panes: Partial<TabPanes>; result: TabMoveResult }

type CreateTabId = () => string

export function clampIndex(index: number, length: number) {
  if (length <= 0) {
    return 0
  }

  return clamp(index, 0, length - 1)
}

function withPane(panes: TabPanes, paneId: PaneId, pane: PaneTabs): TabPanes {
  return { ...panes, [paneId]: pane }
}

function keepsOrder(tabs: TabState[], original: TabState[]) {
  return tabs.every((item, index) => item.id === original[index]?.id)
}

function reorderTab(
  source: PaneTabs,
  sourceIndex: number,
  { sourcePaneId, tabId, destinationIndex }: TabMoveRequest,
): PlannedTabMove | null {
  const outsidePane = destinationIndex < 0 || destinationIndex > source.tabs.length
  if (outsidePane) {
    return null
  }
  const insertionIndex = Math.min(destinationIndex, source.tabs.length - 1)
  const tabs = moveItem(source.tabs, sourceIndex, insertionIndex)
  if (keepsOrder(tabs, source.tabs)) {
    return null
  }
  const activeTabId = source.tabs[source.activeTabIndex]?.id
  const activeTabIndex = tabs.findIndex((item) => item.id === activeTabId)
  return {
    panes: { [sourcePaneId]: { ...source, tabs, activeTabIndex } },
    result: { kind: 'reorder', paneId: sourcePaneId, tabId },
  }
}

function sourceActiveIndexAfterRemoval(
  source: PaneTabs,
  sourceIndex: number,
  remaining: TabState[],
) {
  if (sourceIndex === source.activeTabIndex) {
    return Math.min(sourceIndex, remaining.length - 1)
  }
  const sourceActiveTabId = source.tabs[source.activeTabIndex]?.id
  return remaining.findIndex((tab) => tab.id === sourceActiveTabId)
}

function canTransferTab(source: PaneTabs, destination: PaneTabs, destinationIndex: number) {
  return (
    source.tabs.length > 1 && destinationIndex >= 0 && destinationIndex <= destination.tabs.length
  )
}

function transferTab(
  source: PaneTabs,
  destination: PaneTabs,
  sourceIndex: number,
  request: TabMoveRequest,
  createDestinationTabId: CreateTabId,
): PlannedTabMove | null {
  const { sourcePaneId, tabId, destinationPaneId, destinationIndex } = request
  if (!canTransferTab(source, destination, destinationIndex)) {
    return null
  }
  const sourceTabs = source.tabs.filter((tab) => tab.id !== tabId)
  const destinationTabId = createDestinationTabId()
  const destinationTabs = [...destination.tabs]
  destinationTabs.splice(destinationIndex, 0, { ...source.tabs[sourceIndex], id: destinationTabId })
  return {
    panes: {
      [sourcePaneId]: {
        ...source,
        tabs: sourceTabs,
        activeTabIndex: sourceActiveIndexAfterRemoval(source, sourceIndex, sourceTabs),
      },
      [destinationPaneId]: {
        ...destination,
        tabs: destinationTabs,
        activeTabIndex: destinationIndex,
      },
    },
    result: {
      kind: 'transfer',
      sourcePaneId,
      destinationPaneId,
      sourceActiveChanged: sourceIndex === source.activeTabIndex,
      destinationTabId,
    },
  }
}

export function planTabMove(
  panes: TabPanes,
  request: TabMoveRequest,
  createDestinationTabId: CreateTabId,
): PlannedTabMove | null {
  const source = panes[request.sourcePaneId]
  const sourceIndex = source.tabs.findIndex((tab) => tab.id === request.tabId)
  if (sourceIndex === -1) {
    return null
  }
  if (!Number.isInteger(request.destinationIndex)) {
    return null
  }
  if (request.sourcePaneId === request.destinationPaneId) {
    return reorderTab(source, sourceIndex, request)
  }
  const destination = panes[request.destinationPaneId]
  return transferTab(source, destination, sourceIndex, request, createDestinationTabId)
}

export function withMovedTabs(panes: TabPanes, move: PlannedTabMove): TabPanes {
  return { ...panes, ...move.panes }
}

export function withHydratedPane(
  panes: TabPanes,
  paneId: PaneId,
  pane: PaneTabs,
  fallbackTabs: () => TabState[],
): TabPanes {
  const hasTabs = pane.tabs.length > 0
  const tabs = hasTabs ? pane.tabs : fallbackTabs()
  return withPane(panes, paneId, {
    activeTabIndex: clampIndex(pane.activeTabIndex, pane.tabs.length),
    tabs,
  })
}

export function withAddedTab(
  panes: TabPanes,
  paneId: PaneId,
  tab: TabState,
  activate: boolean,
): TabPanes {
  const pane = panes[paneId]
  const tabs = [...pane.tabs, tab]
  const activeTabIndex = activate ? tabs.length - 1 : pane.activeTabIndex
  return withPane(panes, paneId, { tabs, activeTabIndex })
}

function activeIndexAfterClose(pane: PaneTabs, closingIndex: number, remainingCount: number) {
  if (closingIndex < pane.activeTabIndex) {
    return pane.activeTabIndex - 1
  }
  if (closingIndex === pane.activeTabIndex) {
    return Math.min(pane.activeTabIndex, remainingCount - 1)
  }
  return pane.activeTabIndex
}

function closableTabIndex(pane: PaneTabs, tabId: string) {
  const isLastTab = pane.tabs.length <= 1
  if (isLastTab) {
    return -1
  }
  const closingIndex = pane.tabs.findIndex((tab) => tab.id === tabId)
  if (closingIndex === -1) {
    return -1
  }
  return pane.tabs[closingIndex].locked ? -1 : closingIndex
}

export function withClosedTab(panes: TabPanes, paneId: PaneId, tabId: string): TabPanes | null {
  const pane = panes[paneId]
  const closingIndex = closableTabIndex(pane, tabId)
  if (closingIndex === -1) {
    return null
  }

  const tabs = pane.tabs.filter((tab) => tab.id !== tabId)
  const activeTabIndex = activeIndexAfterClose(pane, closingIndex, tabs.length)
  return withPane(panes, paneId, { tabs, activeTabIndex: clampIndex(activeTabIndex, tabs.length) })
}

export function withTabLocked(
  panes: TabPanes,
  paneId: PaneId,
  tabId: string,
  locked: boolean,
): TabPanes {
  const pane = panes[paneId]
  const tabs = pane.tabs.map((tab) => (tab.id === tabId ? { ...tab, locked } : tab))
  return withPane(panes, paneId, { ...pane, tabs })
}

export function withActiveTab(panes: TabPanes, paneId: PaneId, tabId: string): TabPanes | null {
  const pane = panes[paneId]
  const index = pane.tabs.findIndex((tab) => tab.id === tabId)
  if (index === -1) {
    return null
  }
  if (index === pane.activeTabIndex) {
    return null
  }
  return withPane(panes, paneId, { ...pane, activeTabIndex: index })
}

export function withPatchedActiveTab(panes: TabPanes, paneId: PaneId, patch: TabPatch): TabPanes {
  const pane = panes[paneId]
  const tabs = pane.tabs.map((tab, index) =>
    index === pane.activeTabIndex ? { ...tab, ...patch } : tab,
  )
  return withPane(panes, paneId, { ...pane, tabs })
}
