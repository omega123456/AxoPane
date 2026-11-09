import { create } from 'zustand'
import { applyIconStates, requestVisibleIcons } from '@/stores/panes/icon-actions'
import { applyItemCountEvents, requestVisibleItemCounts } from '@/stores/panes/item-count-actions'
import {
  clearFilter,
  finalizeListing,
  goBack,
  goForward,
  goUp,
  initialize,
  navigatePane,
  reloadPane,
  reset,
  setActivePane,
  setFilterDraft,
  setFocusedEntry,
  setScrollPosition,
  setShowHiddenFiles,
  setSort,
} from '@/stores/panes/navigation-actions'
import { defaultPanesData } from '@/stores/panes/pane-state'
import { applyDirPatch, applySessionPatch } from '@/stores/panes/patch-actions'
import {
  applySizeStates,
  calculateAllFolderSizes,
  refreshSizes,
  requestManualSize,
  setFolderSizeIndexStatus,
} from '@/stores/panes/size-actions'
import { bindPanesStore } from '@/stores/panes/store-handle'
import type { PanesStore } from '@/stores/panes/store-types'
import {
  closeTab,
  closeTabsBeside,
  moveTab,
  openTabFromPath,
  setTabLocked,
  switchTab,
} from '@/stores/panes/tab-actions'
import {
  ensureTreeChildren,
  revealPath,
  revealTreeHiddenLevels,
  revealTreeSiblings,
  setVolumes,
  toggleTreeNode,
} from '@/stores/panes/tree-actions'

export type { TreeFoldReveals, TreeNodeState } from '@/stores/panes/pane-state'
export { getParentPath } from '@/stores/panes/pane-paths'
export {
  AUTO_FOLDER_SIZE_MAX_DIRECTORIES,
  autoFolderSizeDisabledForPane,
  paneDirectoryCount,
} from '@/stores/panes/size-requests'
export { indexEntriesById } from '@/stores/panes/listing-runtime'
export { schedulePersistSession } from '@/stores/panes/session-persistence'
export { initializePanes } from '@/stores/panes/tree-actions'

export const usePanesStore = create<PanesStore>(() => ({
  ...defaultPanesData(),
  initialize,
  setActivePane,
  reloadPane,
  navigatePane,
  goUp,
  goBack,
  goForward,
  openTabFromPath,
  closeTab,
  closeTabsBeside,
  setTabLocked,
  switchTab,
  moveTab,
  refreshSizes,
  finalizeListing,
  applyDirPatch,
  applySessionPatch,
  setSort,
  setFilterDraft,
  clearFilter,
  setFocusedEntry,
  setScrollPosition,
  applySizeStates,
  requestVisibleItemCounts,
  applyItemCountEvents,
  requestVisibleIcons,
  applyIconStates,
  setFolderSizeIndexStatus,
  setVolumes,
  setShowHiddenFiles,
  requestManualSize,
  calculateAllFolderSizes,
  ensureTreeChildren,
  revealPath,
  toggleTreeNode,
  revealTreeSiblings,
  revealTreeHiddenLevels,
  reset,
}))

bindPanesStore(usePanesStore)

export function navigateActivePane(path: string) {
  const panes = usePanesStore.getState()
  void panes.navigatePane(panes.activePaneId, path)
}

export function openTabInActivePane(path: string) {
  const panes = usePanesStore.getState()
  void panes.openTabFromPath(panes.activePaneId, path)
}
