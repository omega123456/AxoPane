import {
  closeTabContextAction,
  closeTabsBesideContextAction,
  navigateContextAction,
  moveTabContextAction,
  setTabLockedContextAction,
} from '@/lib/context-menu/context-menu-actions'
import type {
  ContextMenuContent,
  ContextMenuSection,
  ContextMenuTarget,
} from '@/lib/types/context-menu'
import { commandLabels } from '@/lib/keymap'
import { useLayoutStore } from '@/stores/layout-store'
import { usePanesStore } from '@/stores/panes-store'
import { useTabsStore, type TabState } from '@/stores/tabs-store'
import { useConfigStore } from '@/stores/config-store'
import type { PaneId } from '@/types/pane'
import { pathsMatch } from '@/lib/path-compare'
import { otherPane } from '@/lib/pane-id'

import { customRow, favouriteRow, section } from '@/lib/context-menu/menu-rows'

function hasUnlockedTab(tabList: TabState[]) {
  return tabList.some((item) => !item.locked)
}

function tabPositionFlags(tabList: TabState[], tabIndex: number) {
  const tabMissing = tabIndex === -1
  return {
    cannotMoveLeft: tabIndex <= 0,
    cannotMoveRight: tabMissing || tabIndex >= tabList.length - 1,
    cannotMoveToOtherPane: tabMissing || tabList.length <= 1,
    nothingToCloseLeft: !hasUnlockedTab(tabList.slice(0, Math.max(tabIndex, 0))),
    nothingToCloseRight: tabMissing || !hasUnlockedTab(tabList.slice(tabIndex + 1)),
  }
}

type TabTarget = Extract<ContextMenuTarget, { kind: 'tab' }>
type TabPositionFlags = ReturnType<typeof tabPositionFlags>

type TabPosition = {
  tabIndex: number
  flags: TabPositionFlags
}

function moveTabSection(
  paneId: PaneId,
  target: TabTarget,
  { tabIndex, flags }: TabPosition,
): ContextMenuSection {
  const destinationPaneId = otherPane(paneId)
  const destination = useTabsStore.getState().panes[destinationPaneId]
  return section('move-tab', [
    customRow({
      id: `move-tab-within-left-${target.tabId}`,
      label: 'Move tab left',
      action: moveTabContextAction(target.tabId, paneId, tabIndex - 1),
      disabled: flags.cannotMoveLeft,
      icon: { kind: 'app', name: 'arrow-left' },
    }),
    customRow({
      id: `move-tab-within-right-${target.tabId}`,
      label: 'Move tab right',
      action: moveTabContextAction(target.tabId, paneId, tabIndex + 1),
      disabled: flags.cannotMoveRight,
      icon: { kind: 'app', name: 'arrow-right' },
    }),
    customRow({
      id: `move-tab-${destinationPaneId}-${target.tabId}`,
      label: `Move tab to ${destinationPaneId} pane`,
      action: moveTabContextAction(target.tabId, destinationPaneId, destination.tabs.length),
      disabled: flags.cannotMoveToOtherPane,
      hidden: useLayoutStore.getState().defaultPaneMode === 'single',
      icon: { kind: 'app', name: `panel-${destinationPaneId}` },
    }),
  ])
}

function closeTabRows(
  target: TabTarget,
  cannotClose: boolean | undefined,
  flags: TabPositionFlags,
) {
  return [
    customRow({
      id: `close-tab-${target.tabId}`,
      label: 'Close tab',
      action: closeTabContextAction(target.tabId),
      disabled: cannotClose,
      danger: true,
      icon: { kind: 'app', name: 'close-tab' },
    }),
    customRow({
      id: `close-tabs-left-${target.tabId}`,
      label: 'Close tabs to the left',
      action: closeTabsBesideContextAction(target.tabId, 'left'),
      disabled: flags.nothingToCloseLeft,
      danger: true,
    }),
    customRow({
      id: `close-tabs-right-${target.tabId}`,
      label: 'Close tabs to the right',
      action: closeTabsBesideContextAction(target.tabId, 'right'),
      disabled: flags.nothingToCloseRight,
      danger: true,
    }),
  ]
}

export function buildTabContent(paneId: PaneId, target: TabTarget): ContextMenuContent {
  const tabs = useTabsStore.getState().panes[paneId]
  const tab = tabs.tabs.find((item) => item.id === target.tabId)
  const tabIndex = tabs.tabs.findIndex((item) => item.id === target.tabId)
  const path = tab?.path ?? usePanesStore.getState().panes[paneId].path
  const locked = tab?.locked
  const flags = tabPositionFlags(tabs.tabs, tabIndex)
  const cannotClose = tabs.tabs.length <= 1 || locked
  const isFavourite = useConfigStore.getState().favourites.some((item) => pathsMatch(item, path))

  return {
    topStrip: [],
    sections: [
      section('primary', [
        customRow({
          id: `open-tab-other-${target.tabId}`,
          label: commandLabels.openInOtherPane,
          action: navigateContextAction(path, 'other-pane'),
          strong: true,
          icon: { kind: 'app', name: 'open-in-other-pane' },
        }),
      ]),
      moveTabSection(paneId, target, { tabIndex, flags }),
      section('footer', [
        favouriteRow(path, isFavourite),
        customRow({
          id: `lock-tab-${target.tabId}`,
          label: locked ? 'Unlock tab' : 'Lock tab',
          action: setTabLockedContextAction(target.tabId, !locked),
        }),
        ...closeTabRows(target, cannotClose, flags),
      ]),
    ],
  }
}
