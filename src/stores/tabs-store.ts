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
}

type PaneTabs = {
  activeTabIndex: number
  tabs: TabState[]
}

type TabPatch = Partial<Pick<TabState, 'path' | 'sortKey' | 'sortDirection' | 'filter' | 'viewMode'>>
type NewTabState = Omit<TabState, 'id' | 'viewMode'> & { viewMode?: PaneViewMode }

type TabsStore = {
  panes: Record<PaneId, PaneTabs>
  hydrate: (paneId: PaneId, pane: PaneTabs) => void
  addTab: (paneId: PaneId, tab: NewTabState, options?: { activate?: boolean }) => string
  closeTab: (paneId: PaneId, tabId: string) => void
  setActiveTab: (paneId: PaneId, tabId: string) => void
  patchActiveTab: (paneId: PaneId, patch: TabPatch) => void
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

export function tabLabel(tab: TabState) {
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
        { ...tab, id, viewMode: tab.viewMode ?? useLayoutStore.getState().defaultViewMode },
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
      if (closingIndex === -1) {
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
    })),
  }
}
