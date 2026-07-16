import type { MouseEvent, ReactNode } from 'react'
import { buildContextMenuContent } from '@/components/menus/menu-definitions'
import { LockIcon, PlusIcon, XIcon } from '@/components/icons'
import { LocationIcon } from '@/components/icons/LocationIcon'
import { detectPlatformOs } from '@/lib/keymap'
import {
  useTabDragState,
  useTabSortable,
  useTabStripDropTarget,
} from '@/components/pane/TabDragDropProvider'
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
  const volumes = usePanesStore((state) => state.volumes)
  const switchTab = usePanesStore((state) => state.switchTab)
  const closeTab = usePanesStore((state) => state.closeTab)
  const openTabFromPath = usePanesStore((state) => state.openTabFromPath)
  const openMenu = useContextMenuStore((state) => state.openMenu)
  const os = detectPlatformOs()
  const dragState = useTabDragState()
  const { ref: stripRef } = useTabStripDropTarget(paneId)

  const canClose = paneTabs.tabs.length > 1

  function onCloseClick(event: MouseEvent<HTMLButtonElement>, tabId: string) {
    event.stopPropagation()
    void closeTab(paneId, tabId)
  }

  function onTabAuxClick(event: MouseEvent<HTMLDivElement>, tabId: string) {
    if (event.button !== 1 || !canClose) return
    event.preventDefault()
    void closeTab(paneId, tabId)
  }

  return (
    <div
      ref={stripRef}
      data-tab-strip={paneId}
      className={`flex h-tabs items-stretch gap-0.5 overflow-x-auto bg-light-panel px-1 pt-1 dark:bg-dark-panel ${dragState.isInvalid ? 'cursor-not-allowed' : ''}`}
    >
      {paneTabs.tabs.map((tab, index) => {
        const tabIsActive = index === paneTabs.activeTabIndex
        return (
          <SortableTab
            key={tab.id}
            paneId={paneId}
            tabId={tab.id}
            index={index}
            onAuxClick={(event) => onTabAuxClick(event, tab.id)}
            onContextMenu={(event) => {
              event.preventDefault()
              openMenu({
                paneId,
                title: tabLabel(tab),
                chip: 'TAB',
                x: event.clientX,
                y: event.clientY,
                ...buildContextMenuContent(paneId, { kind: 'tab', tabId: tab.id }, os),
              })
            }}
            className={`group inline-flex min-w-tab-min max-w-tab flex-1 items-center gap-2 rounded-t-tab pl-3 pr-2 text-row transition-colors ${
              tabIsActive
                ? 'bg-light-surface dark:bg-dark-surface'
                : 'text-light-text-soft hover:bg-light-hover dark:text-dark-text-soft dark:hover:bg-dark-hover'
            } ${
              tabIsActive && isActive
                ? 'text-accent-blue-light dark:text-accent-blue'
                : tabIsActive
                  ? 'text-light-text dark:text-dark-text'
                  : ''
            }`}
          >
            {(handleRef) => (
              <>
                <LocationIcon path={tab.path} volumes={volumes} />
                <button
                  ref={handleRef}
                  data-tab-id={tab.id}
                  data-tab-label-id={tab.id}
                  type="button"
                  role="tab"
                  aria-selected={tabIsActive}
                  aria-label={`Tab ${tabLabel(tab)} in ${title}`}
                  onClick={() => void switchTab(paneId, tab.id)}
                  className="min-w-0 flex-1 cursor-pointer truncate rounded-sm py-1 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border"
                >
                  {tabLabel(tab)}
                </button>
                {tab.locked ? (
                  <span
                    aria-label={`Locked tab ${tabLabel(tab)} in ${title}`}
                    className="inline-flex h-5 w-5 shrink-0 items-center justify-center text-light-text-muted dark:text-dark-text-muted"
                  >
                    <LockIcon className="h-3.5 w-3.5" />
                  </span>
                ) : canClose ? (
                  <button
                    type="button"
                    aria-label={`Close tab ${tabLabel(tab)} in ${title}`}
                    onClick={(event) => onCloseClick(event, tab.id)}
                    className="inline-flex h-5 w-5 shrink-0 cursor-pointer items-center justify-center rounded-tab text-light-text-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border hover:bg-light-hover hover:text-light-text dark:text-dark-text-muted dark:hover:bg-dark-hover dark:hover:text-dark-text"
                  >
                    <XIcon className="h-3.5 w-3.5" />
                  </button>
                ) : null}
              </>
            )}
          </SortableTab>
        )
      })}
      <button
        type="button"
        aria-label={`New tab in ${title}`}
        onClick={() => void openTabFromPath(paneId, currentPath)}
        className="my-auto inline-flex h-7 w-7 shrink-0 cursor-pointer items-center justify-center rounded-tab text-light-text-soft focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border hover:bg-light-hover hover:text-light-text dark:text-dark-text-soft dark:hover:bg-dark-hover dark:hover:text-dark-text"
      >
        <PlusIcon className="h-4 w-4" />
      </button>
    </div>
  )
}

type SortableTabProps = {
  paneId: PaneId
  tabId: string
  index: number
  children: (handleRef: (element: Element | null) => void) => ReactNode
  onAuxClick: (event: MouseEvent<HTMLDivElement>) => void
  onContextMenu: (event: MouseEvent<HTMLDivElement>) => void
  className: string
}

function SortableTab({
  paneId,
  tabId,
  index,
  children,
  onAuxClick,
  onContextMenu,
  className,
}: SortableTabProps) {
  const { handleRef, isDragSource, ref } = useTabSortable(paneId, tabId, index)

  return (
    <div
      ref={ref}
      data-tab-id={tabId}
      onAuxClick={onAuxClick}
      onContextMenu={onContextMenu}
      className={`${className} ${isDragSource ? 'cursor-grabbing opacity-80 shadow-float' : ''}`}
    >
      {children(handleRef)}
    </div>
  )
}
