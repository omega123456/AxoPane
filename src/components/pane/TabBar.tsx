import type { MouseEvent } from 'react'
import { buildContextMenuItems } from '@/components/menus/menu-definitions'
import { PlusIcon, XIcon } from '@/components/icons'
import { detectPlatformOs } from '@/lib/keymap'
import { useContextMenuStore } from '@/stores/context-menu-store'
import { usePanesStore } from '@/stores/panes-store'
import { tabLabel, useTabsStore } from '@/stores/tabs-store'
import type { PaneId } from '@/types/pane'

type TabBarProps = {
  paneId: PaneId
  title: string
  currentPath: string
  isActive: boolean
}

export function TabBar({ paneId, title, currentPath, isActive }: TabBarProps) {
  const paneTabs = useTabsStore((state) => state.panes[paneId])
  const switchTab = usePanesStore((state) => state.switchTab)
  const closeTab = usePanesStore((state) => state.closeTab)
  const openTabFromPath = usePanesStore((state) => state.openTabFromPath)
  const openMenu = useContextMenuStore((state) => state.openMenu)
  const os = detectPlatformOs()

  const canClose = paneTabs.tabs.length > 1

  function onCloseClick(event: MouseEvent<HTMLButtonElement>, tabId: string) {
    event.stopPropagation()
    void closeTab(paneId, tabId)
  }

  return (
    <div className="flex h-tabs items-center gap-2 overflow-x-auto border-b border-light-border bg-light-panel px-3 dark:border-dark-border dark:bg-dark-panel">
      {paneTabs.tabs.map((tab, index) => {
        const tabIsActive = index === paneTabs.activeTabIndex
        return (
          <div
            key={tab.id}
            onContextMenu={(event) => {
              event.preventDefault()
              openMenu({
                paneId,
                title: `Tab menu for ${tabLabel(tab)}`,
                x: event.clientX,
                y: event.clientY,
                items: buildContextMenuItems(paneId, { kind: 'tab', tabId: tab.id }, os),
              })
            }}
            className={`group inline-flex shrink-0 items-center gap-1 rounded-tab border px-2 py-1 text-row ${
              tabIsActive && isActive
                ? 'border-accent-blue-border bg-accent-blue-soft text-accent-blue-light dark:text-accent-blue'
                : tabIsActive
                  ? 'border-accent-blue-border bg-accent-blue-soft text-light-text-soft dark:text-dark-text-soft'
                  : 'border-light-border bg-light-surface text-light-text-soft dark:border-dark-border dark:bg-dark-surface dark:text-dark-text-soft'
            }`}
          >
            <button
              type="button"
              role="tab"
              aria-selected={tabIsActive}
              aria-label={`Tab ${tabLabel(tab)} in ${title}`}
              onClick={() => void switchTab(paneId, tab.id)}
              className="max-w-popover truncate rounded-tab px-1 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border"
            >
              {tabLabel(tab)}
            </button>
            {canClose ? (
              <button
                type="button"
                aria-label={`Close tab ${tabLabel(tab)} in ${title}`}
                onClick={(event) => onCloseClick(event, tab.id)}
                className="inline-flex h-4 w-4 items-center justify-center rounded-tab text-light-text-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border hover:bg-light-hover dark:text-dark-text-muted dark:hover:bg-dark-hover"
              >
                <XIcon className="h-3 w-3" />
              </button>
            ) : null}
          </div>
        )
      })}
      <button
        type="button"
        aria-label={`New tab in ${title}`}
        onClick={() => void openTabFromPath(paneId, currentPath)}
        className="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-tab border border-light-border bg-light-surface text-light-text-soft focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:border-dark-border dark:bg-dark-surface dark:text-dark-text-soft"
      >
        <PlusIcon className="h-4 w-4" />
      </button>
    </div>
  )
}
