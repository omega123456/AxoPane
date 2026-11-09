import { create } from 'zustand'
import type { SessionPane } from '@/lib/types/ipc'
import { resolvePaneViewMode, type PaneViewMode } from '@/lib/pane-view'
import { pathsMatch } from '@/lib/path-compare'
import { useLayoutStore } from '@/stores/layout-store'
import type { PaneId } from '@/types/pane'
import {
  planTabMove,
  withActiveTab,
  withAddedTab,
  withClosedTab,
  withHydratedPane,
  withMovedTabs,
  withPatchedActiveTab,
  withTabLocked,
  type PaneTabs,
  type TabMoveRequest,
  type TabMoveResult,
  type TabPanes,
  type TabPatch,
  type TabState,
} from '@/stores/tabs/tab-transitions'
import { DEFAULT_SESSION_PATH } from '@/stores/panes/pane-state'

export type { PaneTabs, TabMoveRequest, TabMoveResult, TabState }

type NewTabState = Omit<TabState, 'id' | 'viewMode' | 'locked'> & {
  viewMode?: PaneViewMode
  locked?: boolean
}

type TabsStore = {
  panes: TabPanes
  hydrate: (paneId: PaneId, pane: PaneTabs) => void
  addTab: (paneId: PaneId, tab: NewTabState, options?: { activate?: boolean }) => string
  closeTab: (paneId: PaneId, tabId: string) => void
  setTabLocked: (paneId: PaneId, tabId: string, locked: boolean) => void
  setActiveTab: (paneId: PaneId, tabId: string) => void
  patchActiveTab: (paneId: PaneId, patch: TabPatch) => void
  moveTab: (request: TabMoveRequest) => TabMoveResult
  reset: () => void
}

const NO_TAB_MOVE: TabMoveResult = { kind: 'none' }
const TRAILING_SEPARATORS = /[\\/]+$/
const PATH_SEPARATOR = /[\\/]/

let tabCounter = 0

export function createTabId(paneId: PaneId) {
  tabCounter += 1
  return `${paneId}-tab-${tabCounter}`
}

function lastLabel(path: string) {
  const trimmed = (path ?? '').replace(TRAILING_SEPARATORS, '')
  const segment = trimmed.split(PATH_SEPARATOR).filter(Boolean).at(-1)
  return segment ?? trimmed ?? path
}

export function tabLabel(tab: Pick<TabState, 'path'>) {
  return lastLabel(tab.path)
}

function defaultViewMode() {
  return useLayoutStore.getState().defaultViewMode
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
        viewMode: defaultViewMode(),
        locked: false,
      },
    ],
  }
}

function defaultState() {
  return {
    panes: {
      left: singleTabPane(DEFAULT_SESSION_PATH),
      right: singleTabPane(DEFAULT_SESSION_PATH),
    },
  }
}

function completedTab(id: string, tab: NewTabState): TabState {
  return {
    ...tab,
    id,
    viewMode: tab.viewMode ?? defaultViewMode(),
    locked: tab.locked ?? false,
  }
}

function panesChange(panes: TabPanes | null) {
  return panes ? { panes } : null
}

export const useTabsStore = create<TabsStore>((set, get) => ({
  ...defaultState(),
  hydrate: (paneId, pane) =>
    set((state) => ({
      panes: withHydratedPane(state.panes, paneId, {
        pane,
        fallbackTabs: () => singleTabPane(DEFAULT_SESSION_PATH).tabs,
      }),
    })),
  addTab: (paneId, tab, options) => {
    const id = createTabId(paneId)
    const activate = options?.activate !== false
    set((state) => ({
      panes: withAddedTab(state.panes, paneId, { tab: completedTab(id, tab), activate }),
    }))
    return id
  },
  closeTab: (paneId, tabId) =>
    set((state) => panesChange(withClosedTab(state.panes, paneId, tabId)) ?? state),
  setTabLocked: (paneId, tabId, locked) =>
    set((state) => ({ panes: withTabLocked(state.panes, paneId, { tabId, locked }) })),
  setActiveTab: (paneId, tabId) =>
    set((state) => panesChange(withActiveTab(state.panes, paneId, tabId)) ?? state),
  patchActiveTab: (paneId, patch) =>
    set((state) => ({ panes: withPatchedActiveTab(state.panes, paneId, patch) })),
  moveTab: (request) => {
    const move = planTabMove(get().panes, request, () => createTabId(request.destinationPaneId))
    if (!move) {
      return NO_TAB_MOVE
    }
    set((state) => ({ panes: withMovedTabs(state.panes, move) }))
    return move.result
  },
  reset: () => set(defaultState()),
}))

export function activeTab(paneId: PaneId): TabState {
  const pane = useTabsStore.getState().panes[paneId]
  return pane.tabs[pane.activeTabIndex] ?? pane.tabs[0]
}

export type TabPath = { tabId: string; path: string }

export function isActiveTabShowingPath(paneId: PaneId, shownPath: string, expected: TabPath) {
  const isActive = activeTab(paneId).id === expected.tabId
  return isActive && pathsMatch(shownPath, expected.path)
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
      viewMode: resolvePaneViewMode(tab.viewMode, defaultViewMode()),
      locked: tab.locked ?? false,
    })),
  }
}
