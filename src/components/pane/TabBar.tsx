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
import { FOCUS_RING } from '@/lib/focus-ring'

type TabBarProps = {
  paneId: PaneId
  title: string
  currentPath: string
}

export function TabBar({ paneId, title, currentPath }: TabBarProps) {
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
      role="group"
      aria-label={`${title} tabs`}
      data-tab-strip={paneId}
      className={`flex h-tabs min-w-0 items-end gap-0.5 overflow-x-auto overflow-y-hidden scrollbar-none ${dragState.isInvalid ? 'cursor-not-allowed' : ''}`}
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
            // The selected tab shares the command bar's layer colour and covers the
            // strip's bottom line, so tab and bar read as one shape.
            className={`group relative inline-flex h-8 w-tab min-w-0 shrink items-center gap-2 rounded-t-control border-x border-t pl-2.75 pr-1.5 text-caption ${
              tabIsActive
                ? 'z-10 border-light-stroke bg-light-layer font-semibold text-light-text dark:border-dark-stroke dark:bg-dark-layer dark:text-dark-text'
                : 'border-transparent text-light-text-secondary hover:bg-light-hover dark:text-dark-text-secondary dark:hover:bg-dark-hover'
            }`}
          >
            {(handleRef) => (
              <>
                <button
                  ref={handleRef}
                  data-tab-id={tab.id}
                  data-tab-label-id={tab.id}
                  type="button"
                  role="tab"
                  aria-selected={tabIsActive}
                  aria-label={`Tab ${tabLabel(tab)} in ${title}`}
                  onClick={() => void switchTab(paneId, tab.id)}
                  // The ::before layer stretches the hit area over the whole tab
                  // (padding included), so a click or drag anywhere on it lands here.
                  className={`flex min-w-0 flex-1 cursor-pointer items-center gap-2 rounded-control text-left before:absolute before:inset-0 before:rounded-t-control ${FOCUS_RING}`}
                >
                  <LocationIcon path={tab.path} volumes={volumes} />
                  <span className="min-w-0 flex-1 truncate">{tabLabel(tab)}</span>
                </button>
                {tab.locked ? (
                  <span
                    aria-label={`Locked tab ${tabLabel(tab)} in ${title}`}
                    className="inline-flex size-5.5 shrink-0 items-center justify-center font-normal text-light-text-secondary dark:text-dark-text-secondary"
                  >
                    <LockIcon className="size-3.5" />
                  </span>
                ) : canClose ? (
                  <button
                    type="button"
                    aria-label={`Close tab ${tabLabel(tab)} in ${title}`}
                    onClick={(event) => onCloseClick(event, tab.id)}
                    // Close shows on the selected tab and on hover, as Windows tabs do.
                    className={`relative z-10 inline-flex size-5.5 shrink-0 cursor-pointer items-center justify-center rounded-control font-normal text-light-text-secondary hover:bg-light-hover hover:text-light-text focus-visible:opacity-100 dark:text-dark-text-secondary dark:hover:bg-dark-hover dark:hover:text-dark-text ${
                      tabIsActive ? '' : 'opacity-0 group-hover:opacity-100'
                    } ${FOCUS_RING}`}
                  >
                    <XIcon className="size-3.5" />
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
        className={`ml-1 inline-flex size-8 shrink-0 cursor-pointer items-center justify-center rounded-control text-light-text-secondary hover:bg-light-hover hover:text-light-text dark:text-dark-text-secondary dark:hover:bg-dark-hover dark:hover:text-dark-text ${FOCUS_RING}`}
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
      className={`${className} ${isDragSource ? 'cursor-grabbing opacity-80 shadow-light-flyout dark:shadow-dark-flyout' : ''}`}
    >
      {children(handleRef)}
    </div>
  )
}
