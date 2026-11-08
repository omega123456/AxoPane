import { log } from '@/lib/app-log-commands'
import { usePanesStore } from '@/stores/panes-store'
import { resetPaneForActiveTab } from '@/stores/panes/listing-load'
import { listingSessions, parkRevealPath } from '@/stores/panes/listing-runtime'
import {
  DEFAULT_TAB_VIEW,
  focusRequested,
  paneResetForTab,
} from '@/stores/panes/listing-transitions'
import { schedulePersistSession } from '@/stores/panes/session-persistence'
import { reportSizeSubscriptions } from '@/stores/panes/size-actions'
import { activeTab, type TabMoveResult, useTabsStore } from '@/stores/tabs-store'
import type { PaneId } from '@/types/pane'

export async function openTabFromPath(
  paneId: PaneId,
  path: string,
  options: { revealPath?: string } = {},
) {
  log.debug('openTabFromPath', { paneId, path })
  parkRevealPath(paneId, options.revealPath)
  const tab = { path, ...DEFAULT_TAB_VIEW }
  useTabsStore.getState().addTab(paneId, { ...tab, locked: false })

  usePanesStore.setState((state) => ({
    ...focusRequested(state, paneId),
    ...paneResetForTab(state, paneId, tab),
  }))

  void listingSessions[paneId].release()
  await usePanesStore.getState().reloadPane(paneId)
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
  schedulePersistSession(usePanesStore.getState().activePaneId)

  if (!wasActive) {
    return
  }

  void reportSizeSubscriptions(paneId, null, [])
  resetPaneForActiveTab(paneId)
  void listingSessions[paneId].release()
  await usePanesStore.getState().reloadPane(paneId)
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
    await usePanesStore.getState().switchTab(paneId, tabId)
  }
  for (const tab of closing) {
    await usePanesStore.getState().closeTab(paneId, tab.id)
  }
}

export function setTabLocked(paneId: PaneId, tabId: string, locked: boolean) {
  useTabsStore.getState().setTabLocked(paneId, tabId, locked)
  schedulePersistSession(usePanesStore.getState().activePaneId)
}

export async function moveTab(
  sourcePaneId: PaneId,
  tabId: string,
  destinationPaneId: PaneId,
  destinationIndex: number,
): Promise<TabMoveResult> {
  const result = useTabsStore
    .getState()
    .moveTab(sourcePaneId, tabId, destinationPaneId, destinationIndex)
  if (result.kind === 'none') {
    return result
  }

  schedulePersistSession(
    result.kind === 'transfer' ? result.destinationPaneId : usePanesStore.getState().activePaneId,
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
  usePanesStore.setState({ activePaneId: result.destinationPaneId })

  for (const paneId of resetPaneIds) {
    void listingSessions[paneId].release()
  }
  await Promise.all(resetPaneIds.map((paneId) => usePanesStore.getState().reloadPane(paneId)))
  return result
}

export async function switchTab(paneId: PaneId, tabId: string) {
  const before = useTabsStore.getState().panes[paneId]
  if (before.tabs[before.activeTabIndex]?.id === tabId) {
    usePanesStore.setState({ activePaneId: paneId })
    return
  }

  useTabsStore.getState().setActiveTab(paneId, tabId)
  schedulePersistSession(paneId)

  const tab = activeTab(paneId)
  usePanesStore.setState((state) => ({
    activePaneId: paneId,
    ...paneResetForTab(state, paneId, tab),
  }))
  void listingSessions[paneId].release()
  await usePanesStore.getState().reloadPane(paneId)
}
