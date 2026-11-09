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
import { tabLabel, useTabsStore, type TabState } from '@/stores/tabs-store'
import { isMiddleButton } from '@/lib/mouse-buttons'
import type { PaneId } from '@/types/pane'
import { FOCUS_RING } from '@/lib/focus-ring'
import { NO_CLASS, TEXT_SECONDARY, HOVER_ON_POINTER } from '@/lib/class-names'

type TabBarProps = {
  paneId: PaneId
  title: string
  currentPath: string
}

function invalidDropCursorClass(isInvalid: boolean) {
  return isInvalid ? 'cursor-not-allowed' : NO_CLASS
}

function tabStateClass(isActive: boolean) {
  return isActive
    ? 'z-10 border-light-stroke bg-light-layer font-semibold text-light-text dark:border-dark-stroke dark:bg-dark-layer dark:text-dark-text'
    : `border-transparent ${TEXT_SECONDARY} ${HOVER_ON_POINTER}`
}

function closeButtonVisibilityClass(isActive: boolean) {
  return isActive ? NO_CLASS : 'opacity-0 group-hover:opacity-100'
}

function dragSourceClass(isDragSource: boolean) {
  return isDragSource
    ? 'cursor-grabbing opacity-80 shadow-light-flyout dark:shadow-dark-flyout'
    : NO_CLASS
}

export function TabBar({ paneId, title, currentPath }: TabBarProps) {
  const paneTabs = useTabsStore((state) => state.panes[paneId])
  const openTabFromPath = usePanesStore((state) => state.openTabFromPath)
  const dragState = useTabDragState()
  const { ref: stripRef } = useTabStripDropTarget(paneId)
  const canClose = paneTabs.tabs.length > 1

  return (
    <div
      ref={stripRef}
      role="group"
      aria-label={`${title} tabs`}
      data-tab-strip={paneId}
      className={`flex h-tabs min-w-0 items-end gap-0.5 overflow-x-auto overflow-y-hidden scrollbar-none ${invalidDropCursorClass(dragState.isInvalid)}`}
    >
      {paneTabs.tabs.map((tab, index) => (
        <PaneTab
          key={tab.id}
          paneId={paneId}
          title={title}
          tab={tab}
          index={index}
          isActive={index === paneTabs.activeTabIndex}
          canClose={canClose}
        />
      ))}
      <button
        type="button"
        aria-label={`New tab in ${title}`}
        onClick={() => void openTabFromPath(paneId, currentPath)}
        className={`ml-1 inline-flex size-8 shrink-0 cursor-pointer items-center justify-center rounded-control ${TEXT_SECONDARY} hover:bg-light-hover hover:text-light-text dark:hover:bg-dark-hover dark:hover:text-dark-text ${FOCUS_RING}`}
      >
        <PlusIcon className="h-4 w-4" />
      </button>
    </div>
  )
}

type PaneTabProps = {
  paneId: PaneId
  title: string
  tab: TabState
  index: number
  isActive: boolean
  canClose: boolean
}

function openTabContextMenu(event: MouseEvent<HTMLDivElement>, paneId: PaneId, tab: TabState) {
  event.preventDefault()
  useContextMenuStore.getState().openMenu({
    paneId,
    title: tabLabel(tab),
    chip: 'TAB',
    x: event.clientX,
    y: event.clientY,
    ...buildContextMenuContent(paneId, { kind: 'tab', tabId: tab.id }, detectPlatformOs()),
  })
}

function PaneTab({ paneId, title, tab, index, isActive, canClose }: PaneTabProps) {
  const volumes = usePanesStore((state) => state.volumes)
  const switchTab = usePanesStore((state) => state.switchTab)
  const closeTab = usePanesStore((state) => state.closeTab)

  function onTabAuxClick(event: MouseEvent<HTMLDivElement>) {
    if (!isMiddleButton(event)) return
    if (!canClose) return
    event.preventDefault()
    void closeTab(paneId, tab.id)
  }

  return (
    <SortableTab
      paneId={paneId}
      tabId={tab.id}
      index={index}
      onAuxClick={onTabAuxClick}
      onContextMenu={(event) => openTabContextMenu(event, paneId, tab)}
      className={`group relative inline-flex h-8 w-tab min-w-0 shrink items-center gap-2 rounded-t-control border-x border-t pl-2.75 pr-1.5 text-caption ${tabStateClass(isActive)}`}
    >
      {(handleRef) => (
        <>
          <button
            ref={handleRef}
            data-tab-id={tab.id}
            data-tab-label-id={tab.id}
            type="button"
            role="tab"
            aria-selected={isActive}
            aria-label={`Tab ${tabLabel(tab)} in ${title}`}
            onClick={() => void switchTab(paneId, tab.id)}
            className={`flex min-w-0 flex-1 cursor-pointer items-center gap-2 rounded-control text-left before:absolute before:inset-0 before:rounded-t-control ${FOCUS_RING}`}
          >
            <LocationIcon path={tab.path} volumes={volumes} />
            <span className="min-w-0 flex-1 truncate">{tabLabel(tab)}</span>
          </button>
          <TabTrailingControl
            label={`${tabLabel(tab)} in ${title}`}
            locked={tab.locked}
            canClose={canClose}
            isActive={isActive}
            onClose={() => void closeTab(paneId, tab.id)}
          />
        </>
      )}
    </SortableTab>
  )
}

type TabTrailingControlProps = {
  label: string
  locked: boolean
  canClose: boolean
  isActive: boolean
  onClose: () => void
}

function TabTrailingControl({
  label,
  locked,
  canClose,
  isActive,
  onClose,
}: TabTrailingControlProps) {
  if (locked) {
    return (
      <span
        aria-label={`Locked tab ${label}`}
        className={`inline-flex size-5.5 shrink-0 items-center justify-center font-normal ${TEXT_SECONDARY}`}
      >
        <LockIcon className="size-3.5" />
      </span>
    )
  }
  if (!canClose) {
    return null
  }
  return (
    <button
      type="button"
      aria-label={`Close tab ${label}`}
      onClick={(event) => {
        event.stopPropagation()
        onClose()
      }}
      className={`relative z-10 inline-flex size-5.5 shrink-0 cursor-pointer items-center justify-center rounded-control font-normal ${TEXT_SECONDARY} hover:bg-light-hover hover:text-light-text focus-visible:opacity-100 dark:hover:bg-dark-hover dark:hover:text-dark-text ${closeButtonVisibilityClass(isActive)} ${FOCUS_RING}`}
    >
      <XIcon className="size-3.5" />
    </button>
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
      className={`${className} ${dragSourceClass(isDragSource)}`}
    >
      {children(handleRef)}
    </div>
  )
}
