import { create } from 'zustand'
import type { SessionPane, SortDirection, SortKey } from '@/lib/types/ipc'
import { resolvePaneViewMode, type PaneViewMode } from '@/lib/pane-view'
import { useLayoutStore } from '@/stores/layout-store'

export type PaneId = 'left' | 'right'

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

  return Math.max(0, Math.min(index, length - 1))
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
      const source = state.panes[sourcePaneId]
      const sourceIndex = source.tabs.findIndex((tab) => tab.id === tabId)
      if (sourceIndex === -1 || !Number.isInteger(destinationIndex)) {
        return state
      }

      if (sourcePaneId === destinationPaneId) {
        if (destinationIndex < 0 || destinationIndex > source.tabs.length) {
          return state
        }

        const tabs = [...source.tabs]
        const [tab] = tabs.splice(sourceIndex, 1)
        const insertionIndex = Math.min(destinationIndex, tabs.length)
        tabs.splice(insertionIndex, 0, tab)
        if (tabs.every((item, index) => item.id === source.tabs[index]?.id)) {
          return state
        }
        const activeTabId = source.tabs[source.activeTabIndex]?.id
        const activeTabIndex = tabs.findIndex((item) => item.id === activeTabId)
        result = { kind: 'reorder', paneId: sourcePaneId, tabId }
        return {
          panes: {
            ...state.panes,
            [sourcePaneId]: { ...source, tabs, activeTabIndex },
          },
        }
      }

      const destination = state.panes[destinationPaneId]
      if (
        source.tabs.length <= 1 ||
        destinationIndex < 0 ||
        destinationIndex > destination.tabs.length
      ) {
        return state
      }

      const sourceActiveChanged = sourceIndex === source.activeTabIndex
      const sourceTabs = source.tabs.filter((tab) => tab.id !== tabId)
      const sourceActiveTabId = source.tabs[source.activeTabIndex]?.id
      const sourceActiveTabIndex = sourceActiveChanged
        ? Math.min(sourceIndex, sourceTabs.length - 1)
        : sourceTabs.findIndex((tab) => tab.id === sourceActiveTabId)
      const destinationTabId = createTabId(destinationPaneId)
      const destinationTabs = [...destination.tabs]
      destinationTabs.splice(destinationIndex, 0, {
        ...source.tabs[sourceIndex],
        id: destinationTabId,
      })
      result = {
        kind: 'transfer',
        sourcePaneId,
        destinationPaneId,
        sourceActiveChanged,
        destinationTabId,
      }
      return {
        panes: {
          ...state.panes,
          [sourcePaneId]: {
            ...source,
            tabs: sourceTabs,
            activeTabIndex: sourceActiveTabIndex,
          },
          [destinationPaneId]: {
            ...destination,
            tabs: destinationTabs,
            activeTabIndex: destinationIndex,
          },
        },
      }
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
