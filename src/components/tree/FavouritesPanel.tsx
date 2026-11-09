import type { DragEvent, MouseEvent } from 'react'
import { ChevronRightIcon } from '@/components/icons'
import { ResizeHandle } from '@/components/shell/ResizeHandle'
import { FavouriteTreeRow, favouriteLabel, type FavouriteRowActions } from './FavouriteTreeRow'
import { buildContextMenuContent } from '@/components/menus/menu-definitions'
import { detectPlatformOs } from '@/lib/keymap'
import { FOCUS_RING_INSET } from '@/lib/focus-ring'
import { useDragResize } from '@/components/shell/use-drag-resize'
import { pathsMatch } from '@/lib/path-compare'
import { TREE_ROW_HEIGHT_PX } from '@/lib/tree-flat'
import { useConfigStore } from '@/stores/config-store'
import { useContextMenuStore } from '@/stores/context-menu-store'
import { useDragStore } from '@/stores/drag-store'
import { FAVOURITES_HEIGHT_MAX, FAVOURITES_HEIGHT_MIN, useLayoutStore } from '@/stores/layout-store'
import { usePanesStore } from '@/stores/panes-store'
import { disclosureChevronClass, SCROLLBAR_THIN } from '@/lib/class-names'
import { canDropOnFavourite, dropAtFavouritesEnd } from './favourite-drop'

function handleFavouritesDragOver(event: DragEvent<HTMLElement>) {
  if (!canDropOnFavourite(useDragStore.getState().drag)) return
  event.preventDefault()
  event.dataTransfer.dropEffect = 'move'
}

async function handleFavouritesDrop(event: DragEvent<HTMLElement>) {
  const drag = useDragStore.getState().drag
  if (!canDropOnFavourite(drag)) return
  event.preventDefault()
  useDragStore.getState().end()
  await dropAtFavouritesEnd(drag)
}

function navigateActivePane(path: string) {
  const panes = usePanesStore.getState()
  void panes.navigatePane(panes.activePaneId, path)
}

function openTabInActivePane(path: string) {
  const panes = usePanesStore.getState()
  void panes.openTabFromPath(panes.activePaneId, path)
}

function openFavouriteContextMenu(path: string, event: MouseEvent) {
  event.preventDefault()
  const paneId = usePanesStore.getState().activePaneId
  useContextMenuStore.getState().openMenu({
    paneId,
    title: useConfigStore.getState().favouriteNames[path] ?? favouriteLabel(path),
    chip: 'DIR',
    x: event.clientX,
    y: event.clientY,
    ...buildContextMenuContent(paneId, { kind: 'favourite', path }, detectPlatformOs()),
  })
}

const favouriteRowActions: FavouriteRowActions = {
  onNavigate: navigateActivePane,
  onOpenTab: openTabInActivePane,
  onContextMenu: openFavouriteContextMenu,
  onRename: (path, name) => void useConfigStore.getState().renameFavourite(path, name),
  onRenameCancel: () => useConfigStore.getState().setRenamingFavourite(null),
}

function FavouritesHeading({ collapsed, count }: { collapsed: boolean; count: number }) {
  return (
    <div onDragOver={handleFavouritesDragOver} onDrop={(event) => void handleFavouritesDrop(event)}>
      <button
        type="button"
        aria-expanded={!collapsed}
        onClick={() => void useConfigStore.getState().setFavouritesCollapsed(!collapsed)}
        className={`flex h-tree-header-first w-full cursor-pointer items-end gap-1.5 rounded-control px-2 pb-1 text-caption text-light-text-tertiary hover:text-light-text dark:text-dark-text-tertiary dark:hover:text-dark-text ${FOCUS_RING_INSET}`}
      >
        <ChevronRightIcon
          aria-hidden="true"
          className={`mb-0.5 size-3 flex-none ${disclosureChevronClass(!collapsed)}`}
        />
        <span>Favourites</span>
        {collapsed ? <span className="ml-auto tabular-nums">{count}</span> : null}
      </button>
    </div>
  )
}

function EmptyFavourites() {
  return (
    <div
      onDragOver={handleFavouritesDragOver}
      onDrop={(event) => void handleFavouritesDrop(event)}
      className="flex h-tree-row items-center rounded-control px-3 text-caption text-light-text-tertiary ring-1 ring-inset ring-light-divider dark:text-dark-text-tertiary dark:ring-dark-divider"
    >
      Drop folders here
    </div>
  )
}

function FavouritesList({ favourites, heightPx }: { favourites: string[]; heightPx: number }) {
  const favouriteNames = useConfigStore((state) => state.favouriteNames)
  const renamingFavourite = useConfigStore((state) => state.renamingFavourite)
  const activePath = usePanesStore((state) => state.panes[state.activePaneId].path)
  return (
    <div
      style={{ maxHeight: `${heightPx}px` }}
      className={`overflow-x-hidden overflow-y-auto overscroll-contain ${SCROLLBAR_THIN}`}
    >
      {favourites.map((path, index) => (
        <FavouriteTreeRow
          key={path}
          path={path}
          name={favouriteNames[path]}
          index={index}
          isCurrent={pathsMatch(activePath, path)}
          isRenaming={renamingFavourite === path}
          actions={favouriteRowActions}
        />
      ))}
    </div>
  )
}

function FavouritesResizeHandle({ heightPx, count }: { heightPx: number; count: number }) {
  const setHeightPx = useLayoutStore((state) => state.setFavouritesHeightPx)
  const shownHeightPx = Math.min(heightPx, count * TREE_ROW_HEIGHT_PX)
  const resizeCallbacks = useDragResize({
    value: shownHeightPx,
    stepSize: TREE_ROW_HEIGHT_PX,
    setValue: setHeightPx,
  })
  return (
    <ResizeHandle
      orientation="horizontal"
      ariaLabel="Resize favourites"
      value={shownHeightPx}
      min={FAVOURITES_HEIGHT_MIN}
      max={FAVOURITES_HEIGHT_MAX}
      showRestLine={false}
      {...resizeCallbacks}
    />
  )
}

function FavouritesBody({ favourites, heightPx }: { favourites: string[]; heightPx: number }) {
  if (favourites.length === 0) {
    return <EmptyFavourites />
  }
  return (
    <>
      <FavouritesList favourites={favourites} heightPx={heightPx} />
      <FavouritesResizeHandle heightPx={heightPx} count={favourites.length} />
    </>
  )
}

export function FavouritesPanel() {
  const favourites = useConfigStore((state) => state.favourites)
  const collapsed = useConfigStore((state) => state.favouritesCollapsed)
  const heightPx = useLayoutStore((state) => state.favouritesHeightPx)

  return (
    <section data-testid="favourites-panel" aria-label="Favourites" className="mx-1 flex-none pb-1">
      <FavouritesHeading collapsed={collapsed} count={favourites.length} />
      {collapsed ? null : <FavouritesBody favourites={favourites} heightPx={heightPx} />}
    </section>
  )
}
