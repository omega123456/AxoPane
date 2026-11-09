import { log } from '@/lib/app-log-commands'
import { panesStore } from '@/stores/panes/store-handle'
import { resetPaneForActiveTab } from '@/stores/panes/listing-load'
import { listingSessions, parkRevealPath } from '@/stores/panes/listing-runtime'
import {
  DEFAULT_TAB_VIEW,
  focusRequested,
  paneResetForTab,
} from '@/stores/panes/listing-transitions'
import { schedulePersistSession } from '@/stores/panes/session-persistence'
import { reportSizeSubscriptions } from '@/stores/panes/size-actions'
import {
  activeTab,
  type TabMoveRequest,
  type TabMoveResult,
  useTabsStore,
} from '@/stores/tabs-store'
import type { PaneId } from '@/types/pane'

async function reloadWithFreshSession(paneId: PaneId) {
  void listingSessions[paneId].release()
  await panesStore().getState().reloadPane(paneId)
}

export async function openTabFromPath(
  paneId: PaneId,
  path: string,
  options: { revealPath?: string } = {},
) {
  log.debug('openTabFromPath', { paneId, path })
  parkRevealPath(paneId, options.revealPath)
  const tab = { path, ...DEFAULT_TAB_VIEW }
  useTabsStore.getState().addTab(paneId, { ...tab, locked: false })

  panesStore().setState((state) => ({
    ...focusRequested(state, paneId),
    ...paneResetForTab(state, paneId, tab),
  }))

  await reloadWithFreshSession(paneId)
}

export async function closeTab(paneId: PaneId, tabId: string) {
  const before = useTabsStore.getState().panes[paneId]
  const closingTab = before.tabs.find((tab) => tab.id === tabId)
  const cannotClose = before.tabs.length <= 1 || !closingTab || closingTab.locked
  if (cannotClose) {
    return
  }

  const wasActive = before.tabs[before.activeTabIndex]?.id === tabId
  useTabsStore.getState().closeTab(paneId, tabId)
  schedulePersistSession(panesStore().getState().activePaneId)

  if (!wasActive) {
    return
  }

  void reportSizeSubscriptions(paneId, null, [])
  resetPaneForActiveTab(paneId)
  await reloadWithFreshSession(paneId)
}

export async function closeTabsBeside(paneId: PaneId, tabId: string, side: 'left' | 'right') {
  const { tabs, activeTabIndex } = useTabsStore.getState().panes[paneId]
  const index = tabs.findIndex((tab) => tab.id === tabId)
  if (index === -1) {
    return
  }

  const besideTabs = side === 'left' ? tabs.slice(0, index) : tabs.slice(index + 1)
  const closing = besideTabs.filter((tab) => !tab.locked)
  const closesActiveTab = closing.some((tab) => tab.id === tabs[activeTabIndex]?.id)
  if (closesActiveTab) {
    await panesStore().getState().switchTab(paneId, tabId)
  }
  for (const tab of closing) {
    await panesStore().getState().closeTab(paneId, tab.id)
  }
}

export function setTabLocked(paneId: PaneId, tabId: string, locked: boolean) {
  useTabsStore.getState().setTabLocked(paneId, tabId, locked)
  schedulePersistSession(panesStore().getState().activePaneId)
}

export async function moveTab(request: TabMoveRequest): Promise<TabMoveResult> {
  const result = useTabsStore.getState().moveTab(request)
  if (result.kind === 'none') {
    return result
  }

  schedulePersistSession(
    result.kind === 'transfer' ? result.destinationPaneId : panesStore().getState().activePaneId,
  )
  if (result.kind === 'reorder') {
    return result
  }

  const resetPaneIds = result.sourceActiveChanged
    ? [result.destinationPaneId, result.sourcePaneId]
    : [result.destinationPaneId]
  for (const paneId of resetPaneIds) {
    resetPaneForActiveTab(paneId)
  }
  panesStore().setState({ activePaneId: result.destinationPaneId })

  await Promise.all(resetPaneIds.map(reloadWithFreshSession))
  return result
}

export async function switchTab(paneId: PaneId, tabId: string) {
  const before = useTabsStore.getState().panes[paneId]
  const activeTabBefore = before.tabs[before.activeTabIndex]
  if (activeTabBefore?.id === tabId) {
    panesStore().setState({ activePaneId: paneId })
    return
  }

  useTabsStore.getState().setActiveTab(paneId, tabId)
  schedulePersistSession(paneId)

  const tab = activeTab(paneId)
  panesStore().setState((state) => ({
    activePaneId: paneId,
    ...paneResetForTab(state, paneId, tab),
  }))
  await reloadWithFreshSession(paneId)
}
