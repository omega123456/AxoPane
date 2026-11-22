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

export type PaneTabList = {
  activeTabIndex: number
  tabs: TabState[]
}

export type PaneTabs = PaneTabList & {
  previouslyActiveTabIds: string[]
}

type TabSelection = Pick<PaneTabs, 'activeTabIndex' | 'previouslyActiveTabIds'>

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

function activeTabId(pane: PaneTabs) {
  return pane.tabs[pane.activeTabIndex]?.id
}

function indexOfTab(tabs: TabState[], tabId: string) {
  return clampIndex(
    tabs.findIndex((tab) => tab.id === tabId),
    tabs.length,
  )
}

function withoutTabId(tabIds: string[], tabId: string) {
  return tabIds.filter((id) => id !== tabId)
}

function historyAfterSwitch(pane: PaneTabs, nextActiveTabId: string) {
  const earlierTabIds = withoutTabId(pane.previouslyActiveTabIds, nextActiveTabId)
  return [...earlierTabIds, activeTabId(pane)]
}

function selectionAfterRemoval(
  pane: PaneTabs,
  removedTabId: string,
  remainingTabs: TabState[],
): TabSelection {
  const history = withoutTabId(pane.previouslyActiveTabIds, removedTabId)
  const currentActiveTabId = activeTabId(pane)
  if (currentActiveTabId !== removedTabId) {
    return {
      activeTabIndex: indexOfTab(remainingTabs, currentActiveTabId),
      previouslyActiveTabIds: history,
    }
  }
  const neighbourTabId = remainingTabs[clampIndex(pane.activeTabIndex, remainingTabs.length)].id
  const nextActiveTabId = history.at(-1) ?? neighbourTabId
  return {
    activeTabIndex: indexOfTab(remainingTabs, nextActiveTabId),
    previouslyActiveTabIds: withoutTabId(history, nextActiveTabId),
  }
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

function canTransferTab(source: PaneTabs, destination: PaneTabs, destinationIndex: number) {
  return (
    source.tabs.length > 1 && destinationIndex >= 0 && destinationIndex <= destination.tabs.length
  )
}

type TabTransferPanes = {
  source: PaneTabs
  destination: PaneTabs
  sourceIndex: number
}

function transferTab(
  transferPanes: TabTransferPanes,
  request: TabMoveRequest,
  createDestinationTabId: CreateTabId,
): PlannedTabMove | null {
  const { source, destination, sourceIndex } = transferPanes
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
        tabs: sourceTabs,
        ...selectionAfterRemoval(source, tabId, sourceTabs),
      },
      [destinationPaneId]: {
        tabs: destinationTabs,
        activeTabIndex: destinationIndex,
        previouslyActiveTabIds: historyAfterSwitch(destination, destinationTabId),
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
  return transferTab({ source, destination, sourceIndex }, request, createDestinationTabId)
}

export function withMovedTabs(panes: TabPanes, move: PlannedTabMove): TabPanes {
  return { ...panes, ...move.panes }
}

export type PaneHydration = {
  pane: PaneTabList
  fallbackTabs: () => TabState[]
}

export function withHydratedPane(
  panes: TabPanes,
  paneId: PaneId,
  { pane, fallbackTabs }: PaneHydration,
): TabPanes {
  const hasTabs = pane.tabs.length > 0
  const tabs = hasTabs ? pane.tabs : fallbackTabs()
  return withPane(panes, paneId, {
    activeTabIndex: clampIndex(pane.activeTabIndex, pane.tabs.length),
    tabs,
    previouslyActiveTabIds: [],
  })
}

export type TabAddition = {
  tab: TabState
  activate: boolean
}

export function withAddedTab(
  panes: TabPanes,
  paneId: PaneId,
  { tab, activate }: TabAddition,
): TabPanes {
  const pane = panes[paneId]
  const tabs = [...pane.tabs, tab]
  if (!activate) {
    return withPane(panes, paneId, { ...pane, tabs })
  }
  return withPane(panes, paneId, {
    tabs,
    activeTabIndex: tabs.length - 1,
    previouslyActiveTabIds: historyAfterSwitch(pane, tab.id),
  })
}

export type TabClone = {
  tabId: string
  cloneId: string
}

export function withClonedTab(
  panes: TabPanes,
  paneId: PaneId,
  { tabId, cloneId }: TabClone,
): TabPanes | null {
  const pane = panes[paneId]
  const sourceIndex = pane.tabs.findIndex((tab) => tab.id === tabId)
  if (sourceIndex === -1) {
    return null
  }
  const cloneIndex = sourceIndex + 1
  const clone = { ...pane.tabs[sourceIndex], id: cloneId, locked: false }
  const tabs = [...pane.tabs.slice(0, cloneIndex), clone, ...pane.tabs.slice(cloneIndex)]
  return withPane(panes, paneId, {
    tabs,
    activeTabIndex: cloneIndex,
    previouslyActiveTabIds: historyAfterSwitch(pane, cloneId),
  })
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
  return withPane(panes, paneId, { tabs, ...selectionAfterRemoval(pane, tabId, tabs) })
}

export type TabLock = {
  tabId: string
  locked: boolean
}

export function withTabLocked(
  panes: TabPanes,
  paneId: PaneId,
  { tabId, locked }: TabLock,
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
  return withPane(panes, paneId, {
    tabs: pane.tabs,
    activeTabIndex: index,
    previouslyActiveTabIds: historyAfterSwitch(pane, tabId),
  })
}

export function withPatchedActiveTab(panes: TabPanes, paneId: PaneId, patch: TabPatch): TabPanes {
  const pane = panes[paneId]
  const tabs = pane.tabs.map((tab, index) =>
    index === pane.activeTabIndex ? { ...tab, ...patch } : tab,
  )
  return withPane(panes, paneId, { ...pane, tabs })
}
