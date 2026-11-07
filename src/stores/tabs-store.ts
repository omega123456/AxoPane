import { create } from 'zustand'
import type { SessionPane, SortDirection, SortKey } from '@/lib/types/ipc'
import { resolvePaneViewMode, type PaneViewMode } from '@/lib/pane-view'
import { useLayoutStore } from '@/stores/layout-store'
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

type PaneTabs = {
  activeTabIndex: number
  tabs: TabState[]
}

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

type TabPatch = Partial<
  Pick<TabState, 'path' | 'sortKey' | 'sortDirection' | 'filter' | 'viewMode'>
>
type NewTabState = Omit<TabState, 'id' | 'viewMode' | 'locked'> & {
  viewMode?: PaneViewMode
  locked?: boolean
}

type TabsStore = {
  panes: Record<PaneId, PaneTabs>
  hydrate: (paneId: PaneId, pane: PaneTabs) => void
  addTab: (paneId: PaneId, tab: NewTabState, options?: { activate?: boolean }) => string
  closeTab: (paneId: PaneId, tabId: string) => void
  setTabLocked: (paneId: PaneId, tabId: string, locked: boolean) => void
  setActiveTab: (paneId: PaneId, tabId: string) => void
  patchActiveTab: (paneId: PaneId, patch: TabPatch) => void
  moveTab: (
    sourcePaneId: PaneId,
    tabId: string,
    destinationPaneId: PaneId,
    destinationIndex: number,
  ) => TabMoveResult
  reset: () => void
}

let tabCounter = 0

export function createTabId(paneId: PaneId) {
  tabCounter += 1
  return `${paneId}-tab-${tabCounter}`
}

function lastLabel(path: string) {
  const trimmed = (path ?? '').replace(/[\\/]+$/, '')
  const segment = trimmed.split(/[\\/]/).filter(Boolean).at(-1)
  return segment ?? trimmed ?? path
}

export function tabLabel(tab: Pick<TabState, 'path'>) {
  return lastLabel(tab.path)
}

function singleTabPane(path: string): PaneTabs {
  return {
    activeTabIndex: 0,
    tabs: [
      {
        id: createTabId('left'),
        path,
        sortKey: 'name',
        sortDirection: 'asc',
        filter: '',
        viewMode: useLayoutStore.getState().defaultViewMode,
        locked: false,
      },
    ],
  }
}

function defaultState() {
  return {
    panes: {
      left: singleTabPane('.'),
      right: singleTabPane('.'),
    },
  }
}

function clampIndex(index: number, length: number) {
  if (length <= 0) {
    return 0
  }

  return clamp(index, 0, length - 1)
}

type TabMoveRequest = {
  sourcePaneId: PaneId
  tabId: string
  destinationPaneId: PaneId
  destinationIndex: number
}

type PlannedTabMove = { panes: Partial<Record<PaneId, PaneTabs>>; result: TabMoveResult }

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
  if (tabs.every((item, index) => item.id === source.tabs[index]?.id)) {
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
  { sourcePaneId, tabId, destinationPaneId, destinationIndex }: TabMoveRequest,
): PlannedTabMove | null {
  if (!canTransferTab(source, destination, destinationIndex)) {
    return null
  }
  const sourceTabs = source.tabs.filter((tab) => tab.id !== tabId)
  const destinationTabId = createTabId(destinationPaneId)
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

function planTabMove(
  panes: Record<PaneId, PaneTabs>,
  request: TabMoveRequest,
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
  return transferTab(source, panes[request.destinationPaneId], sourceIndex, request)
}

export const useTabsStore = create<TabsStore>((set) => ({
  ...defaultState(),
  hydrate: (paneId, pane) =>
    set((state) => ({
      panes: {
        ...state.panes,
        [paneId]: {
          activeTabIndex: clampIndex(pane.activeTabIndex, pane.tabs.length),
          tabs: pane.tabs.length > 0 ? pane.tabs : singleTabPane('.').tabs,
        },
      },
    })),
  addTab: (paneId, tab, options) => {
    const id = createTabId(paneId)
    set((state) => {
      const pane = state.panes[paneId]
      const tabs = [
        ...pane.tabs,
        {
          ...tab,
          id,
          viewMode: tab.viewMode ?? useLayoutStore.getState().defaultViewMode,
          locked: tab.locked ?? false,
        },
      ]
      return {
        panes: {
          ...state.panes,
          [paneId]: {
            tabs,
            activeTabIndex: options?.activate === false ? pane.activeTabIndex : tabs.length - 1,
          },
        },
      }
    })
    return id
  },
  closeTab: (paneId, tabId) =>
    set((state) => {
      const pane = state.panes[paneId]
      if (pane.tabs.length <= 1) {
        return state
      }

      const closingIndex = pane.tabs.findIndex((tab) => tab.id === tabId)
      if (closingIndex === -1 || pane.tabs[closingIndex].locked) {
        return state
      }

      const tabs = pane.tabs.filter((tab) => tab.id !== tabId)
      let activeTabIndex = pane.activeTabIndex
      if (closingIndex < pane.activeTabIndex) {
        activeTabIndex -= 1
      } else if (closingIndex === pane.activeTabIndex) {
        activeTabIndex = Math.min(pane.activeTabIndex, tabs.length - 1)
      }

      return {
        panes: {
          ...state.panes,
          [paneId]: {
            tabs,
            activeTabIndex: clampIndex(activeTabIndex, tabs.length),
          },
        },
      }
    }),
  setTabLocked: (paneId, tabId, locked) =>
    set((state) => {
      const pane = state.panes[paneId]
      const tabs = pane.tabs.map((tab) => (tab.id === tabId ? { ...tab, locked } : tab))
      return { panes: { ...state.panes, [paneId]: { ...pane, tabs } } }
    }),
  setActiveTab: (paneId, tabId) =>
    set((state) => {
      const pane = state.panes[paneId]
      const index = pane.tabs.findIndex((tab) => tab.id === tabId)
      if (index === -1 || index === pane.activeTabIndex) {
        return state
      }

      return {
        panes: {
          ...state.panes,
          [paneId]: { ...pane, activeTabIndex: index },
        },
      }
    }),
  patchActiveTab: (paneId, patch) =>
    set((state) => {
      const pane = state.panes[paneId]
      const tabs = pane.tabs.map((tab, index) =>
        index === pane.activeTabIndex ? { ...tab, ...patch } : tab,
      )

      return {
        panes: {
          ...state.panes,
          [paneId]: { ...pane, tabs },
        },
      }
    }),
  moveTab: (sourcePaneId, tabId, destinationPaneId, destinationIndex) => {
    let result: TabMoveResult = { kind: 'none' }
    set((state) => {
      const move = planTabMove(state.panes, {
        sourcePaneId,
        tabId,
        destinationPaneId,
        destinationIndex,
      })
      if (!move) {
        return state
      }
      result = move.result
      return { panes: { ...state.panes, ...move.panes } }
    })
    return result
  },
  reset: () => set(defaultState()),
}))

export function activeTab(paneId: PaneId): TabState {
  const pane = useTabsStore.getState().panes[paneId]
  return pane.tabs[pane.activeTabIndex] ?? pane.tabs[0]
}

export function toSessionPane(paneId: PaneId): SessionPane {
  const pane = useTabsStore.getState().panes[paneId]
  return {
    activeTabIndex: pane.activeTabIndex,
    tabs: pane.tabs.map((tab) => ({
      path: tab.path,
      sortKey: tab.sortKey,
      sortDirection: tab.sortDirection,
      filter: tab.filter,
      viewMode: tab.viewMode,
      locked: tab.locked,
    })),
  }
}

export function fromSessionPane(paneId: PaneId, pane: SessionPane): PaneTabs {
  return {
    activeTabIndex: pane.activeTabIndex,
    tabs: pane.tabs.map((tab) => ({
      id: createTabId(paneId),
      path: tab.path,
      sortKey: tab.sortKey,
      sortDirection: tab.sortDirection,
      filter: tab.filter,
      viewMode: resolvePaneViewMode(tab.viewMode, useLayoutStore.getState().defaultViewMode),
      locked: tab.locked ?? false,
    })),
  }
}
