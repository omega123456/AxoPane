import { useMemo, useRef, type DragEvent } from 'react'
import { ChevronRightIcon } from '@/components/icons'
import { ResizeHandle } from '@/components/shell/ResizeHandle'
import { FavouriteTreeRow, favouriteLabel, type FavouriteRowActions } from './FavouriteTreeRow'
import { buildContextMenuContent } from '@/components/menus/menu-definitions'
import { detectPlatformOs } from '@/lib/keymap'
import { FOCUS_RING_INSET } from '@/lib/focus-ring'
import { persistAppConfig } from '@/lib/app-config'
import { pathsMatch } from '@/lib/path-compare'
import { TREE_ROW_HEIGHT_PX } from '@/lib/tree-flat'
import { useConfigStore } from '@/stores/config-store'
import { useContextMenuStore } from '@/stores/context-menu-store'
import { useDragStore } from '@/stores/drag-store'
import { FAVOURITES_HEIGHT_MAX, FAVOURITES_HEIGHT_MIN, useLayoutStore } from '@/stores/layout-store'
import { usePanesStore } from '@/stores/panes-store'

function canDropOnFavourites() {
  const drag = useDragStore.getState().drag
  return drag?.kind === 'favourite' || drag?.items.some((item) => item.isDir) === true
}

function handleFavouritesDragOver(event: DragEvent<HTMLElement>) {
  if (!canDropOnFavourites()) return
  event.preventDefault()
  event.dataTransfer.dropEffect = 'move'
}

/** Drops onto the heading or the empty row append to the end of the list. */
async function handleFavouritesDrop(event: DragEvent<HTMLElement>) {
  if (!canDropOnFavourites()) return
  event.preventDefault()
  const drag = useDragStore.getState().drag
  useDragStore.getState().end()
  const config = useConfigStore.getState()
  if (drag?.kind === 'favourite') {
    await config.reorderFavourite(drag.path, config.favourites.length)
    return
  }
  if (drag?.kind === 'file-transfer') {
    for (const item of drag.items) {
      if (item.isDir) await config.addFavourite(item.path)
    }
  }
}

/**
 * Favourites, fixed above the folder tree so they stay on screen however deep
 * the tree is expanded. The list grows to a user-draggable (persisted) height,
 * then scrolls inside its own area; the heading collapses it to a single row
 * with a count.
 */
export function FavouritesPanel() {
  const favourites = useConfigStore((state) => state.favourites)
  const favouriteNames = useConfigStore((state) => state.favouriteNames)
  const renamingFavourite = useConfigStore((state) => state.renamingFavourite)
  const collapsed = useConfigStore((state) => state.favouritesCollapsed)
  const activePath = usePanesStore((state) => state.panes[state.activePaneId].path)
  const heightPx = useLayoutStore((state) => state.favouritesHeightPx)
  const setHeightPx = useLayoutStore((state) => state.setFavouritesHeightPx)
  // Rows are fixed-height, so the list's rendered height is the stored cap or
  // its content, whichever is smaller. Resizing starts from that, so a short
  // list reacts to the first pixel of an upward drag.
  const shownHeightPx = Math.min(heightPx, favourites.length * TREE_ROW_HEIGHT_PX)
  const dragStartHeight = useRef(shownHeightPx)

  // Stable for the memoized rows; every handler reads current store state.
  const actions = useMemo<FavouriteRowActions>(
    () => ({
      onNavigate: (path) => {
        const panes = usePanesStore.getState()
        void panes.navigatePane(panes.activePaneId, path)
      },
      onOpenTab: (path) => {
        const panes = usePanesStore.getState()
        void panes.openTabFromPath(panes.activePaneId, path)
      },
      onContextMenu: (path, event) => {
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
      },
      onRename: (path, name) => void useConfigStore.getState().renameFavourite(path, name),
      onRenameCancel: () => useConfigStore.getState().setRenamingFavourite(null),
    }),
    [],
  )

  return (
    <section data-testid="favourites-panel" aria-label="Favourites" className="mx-1 flex-none pb-1">
      <div
        onDragOver={handleFavouritesDragOver}
        onDrop={(event) => void handleFavouritesDrop(event)}
      >
        <button
          type="button"
          aria-expanded={!collapsed}
          onClick={() => void useConfigStore.getState().setFavouritesCollapsed(!collapsed)}
          className={`flex h-tree-header-first w-full cursor-pointer items-end gap-1.5 rounded-control px-2 pb-1 text-caption text-light-text-tertiary hover:text-light-text dark:text-dark-text-tertiary dark:hover:text-dark-text ${FOCUS_RING_INSET}`}
        >
          <ChevronRightIcon
            aria-hidden="true"
            className={`mb-0.5 size-3 flex-none ${collapsed ? '' : 'rotate-90'}`}
          />
          <span>Favourites</span>
          {collapsed ? <span className="ml-auto tabular-nums">{favourites.length}</span> : null}
        </button>
      </div>
      {collapsed ? null : favourites.length === 0 ? (
        <div
          onDragOver={handleFavouritesDragOver}
          onDrop={(event) => void handleFavouritesDrop(event)}
          className="flex h-tree-row items-center rounded-control px-3 text-caption text-light-text-tertiary ring-1 ring-inset ring-light-divider dark:text-dark-text-tertiary dark:ring-dark-divider"
        >
          Drop folders here
        </div>
      ) : (
        <div
          // Height is user-draggable runtime geometry; no static token can express it.
          style={{ maxHeight: `${heightPx}px` }}
          className="overflow-x-hidden overflow-y-auto overscroll-contain scrollbar-thin scrollbar-track-transparent scrollbar-thumb-light-text-tertiary dark:scrollbar-thumb-dark-text-tertiary"
        >
          {favourites.map((path, index) => (
            <FavouriteTreeRow
              key={path}
              path={path}
              name={favouriteNames[path]}
              index={index}
              isCurrent={pathsMatch(activePath, path)}
              isRenaming={renamingFavourite === path}
              actions={actions}
            />
          ))}
        </div>
      )}
      {collapsed || favourites.length === 0 ? null : (
        <ResizeHandle
          orientation="horizontal"
          ariaLabel="Resize favourites"
          value={shownHeightPx}
          min={FAVOURITES_HEIGHT_MIN}
          max={FAVOURITES_HEIGHT_MAX}
          showRestLine={false}
          onDragStart={() => {
            dragStartHeight.current = shownHeightPx
          }}
          onResize={(deltaY) => setHeightPx(dragStartHeight.current + deltaY)}
          onStep={(delta) => setHeightPx(shownHeightPx + delta * TREE_ROW_HEIGHT_PX)}
          onCommit={() => void persistAppConfig()}
        />
      )}
    </section>
  )
}
