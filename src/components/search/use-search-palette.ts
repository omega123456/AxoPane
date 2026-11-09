import { useEffect, useMemo, useState, type KeyboardEvent, type RefObject } from 'react'
import { favouriteLabel } from '@/components/tree/FavouriteTreeRow'
import { isSearchEverywhereShortcut } from '@/lib/keymap'
import { handleKeyByTable, hasPrimaryModifier, type KeyHandlerTable } from '@/lib/key-table'
import {
  activateSearchHit,
  completeSearchQuery,
  searchActivationForEvent,
  type SearchActivation,
} from '@/lib/search-actions'
import type { CommandId, FolderSizeIndexStatus, SearchHit, Shortcut } from '@/lib/types/ipc'
import { useElementVirtualizer } from '@/lib/use-element-virtualizer'
import { virtualRowsOrFallback } from '@/lib/virtual-rows'
import { clamp } from '@/lib/numbers'
import { useConfigStore } from '@/stores/config-store'
import { useKeymapStore } from '@/stores/keymap-store'
import { usePanesStore } from '@/stores/panes-store'
import { useRecentFoldersStore } from '@/stores/recent-folders-store'
import { useSearchPaletteStore } from '@/stores/search-palette-store'
import { folderRow, indexAnnouncementAfter, withTrailingSeparator } from './search-palette-model'

export const SEARCH_ROW_HEIGHT = 44
export const SEARCH_PAGE_ROWS = 8
const UNMEASURED_FALLBACK_ROWS = SEARCH_PAGE_ROWS + SEARCH_PAGE_ROWS
const MAX_RECENTS = 6
const FALLBACK_ROW_KEY_PREFIX = 'search-row'
const SEARCH_TRIGGER_SELECTOR = '[data-search-trigger]'

type KeyBindings = Record<CommandId, Shortcut[]>

export type PaletteNavigation = {
  query: string
  searching: boolean
  rows: SearchHit[]
  activeIndex: number
  activeRow: SearchHit | undefined
  keymap: KeyBindings
  inputRef: RefObject<HTMLInputElement | null>
}

export function useSearchPaletteView() {
  return {
    query: useSearchPaletteStore((state) => state.query),
    scope: useSearchPaletteStore((state) => state.scope),
    status: useSearchPaletteStore((state) => state.status),
    items: useSearchPaletteStore((state) => state.items),
    total: useSearchPaletteStore((state) => state.total),
    truncated: useSearchPaletteStore((state) => state.truncated),
    loadingMore: useSearchPaletteStore((state) => state.loadingMore),
    activeIndex: useSearchPaletteStore((state) => state.activeIndex),
    error: useSearchPaletteStore((state) => state.error),
    capabilities: useSearchPaletteStore((state) => state.capabilities),
    focusRequest: useSearchPaletteStore((state) => state.focusRequest),
    indexStatus: usePanesStore((state) => state.folderSizeIndexStatus),
    keymap: useKeymapStore((state) => state.bindings),
  }
}

export function useFolderShortcutRows() {
  const favourites = useConfigStore((state) => state.favourites)
  const favouriteNames = useConfigStore((state) => state.favouriteNames)
  const recentFolders = useRecentFoldersStore((state) => state.folders)
  const favouriteRows = useMemo(
    () => favourites.map((path) => folderRow(path, favouriteNames[path] ?? favouriteLabel(path))),
    [favourites, favouriteNames],
  )
  const recentRows = useMemo(
    () =>
      recentFolders
        .filter((path) => !favourites.includes(path))
        .slice(0, MAX_RECENTS)
        .map((path) => folderRow(path, favouriteLabel(path))),
    [favourites, recentFolders],
  )
  return { favouriteRows, recentRows }
}

type TrackedIndexAnnouncement = {
  status: FolderSizeIndexStatus | null
  announcement: string
}

export function useIndexAnnouncement(status: FolderSizeIndexStatus | null) {
  const [tracked, setTracked] = useState<TrackedIndexAnnouncement>({ status, announcement: '' })
  if (tracked.status !== status) {
    const announcement = indexAnnouncementAfter(tracked.status, status, tracked.announcement)
    setTracked({ status, announcement })
  }
  return tracked.announcement
}

export function useSearchVirtualRows(rowCount: number, listRef: RefObject<HTMLDivElement | null>) {
  const virtualizer = useElementVirtualizer({
    count: rowCount,
    estimateSize: () => SEARCH_ROW_HEIGHT,
    getScrollElement: () => listRef.current,
    overscan: SEARCH_PAGE_ROWS,
  })
  const virtualRows = virtualRowsOrFallback({
    measuredRows: virtualizer.getVirtualItems(),
    rowCount,
    fallbackRowLimit: UNMEASURED_FALLBACK_ROWS,
    rowStart: (index) => index * SEARCH_ROW_HEIGHT,
    keyPrefix: FALLBACK_ROW_KEY_PREFIX,
  })
  return { virtualizer, virtualRows }
}

function isInsidePalette(panel: HTMLElement | null, target: Element) {
  return Boolean(panel?.contains(target)) || target.closest(SEARCH_TRIGGER_SELECTOR) !== null
}

export function useCloseOnOutsidePointer(
  panelRef: RefObject<HTMLDivElement | null>,
  close: () => void,
) {
  useEffect(() => {
    function onPointerDown(event: PointerEvent) {
      if (!isInsidePalette(panelRef.current, event.target as Element)) {
        close()
      }
    }
    document.addEventListener('pointerdown', onPointerDown)
    return () => document.removeEventListener('pointerdown', onPointerDown)
  }, [close, panelRef])
}

export function dismissPalette() {
  const paneId = usePanesStore.getState().activePaneId
  useSearchPaletteStore.getState().close()
  document.querySelector<HTMLElement>(`[data-pane-id="${paneId}"]`)?.focus()
}

export function activatePaletteHit(hit: SearchHit, how: SearchActivation) {
  dismissPalette()
  void activateSearchHit(hit, how)
}

function movePaletteSelection(navigation: PaletteNavigation, delta: number) {
  const { moveActive, setActiveIndex } = useSearchPaletteStore.getState()
  if (navigation.searching) {
    moveActive(delta)
    return
  }
  const rowCount = navigation.rows.length
  if (rowCount === 0) {
    return
  }
  setActiveIndex(clamp(navigation.activeIndex + delta, 0, rowCount - 1))
}

function clearQueryOrDismiss(navigation: PaletteNavigation) {
  if (navigation.query) {
    useSearchPaletteStore.getState().setQuery('')
    return
  }
  dismissPalette()
}

function jumpToLastRow(navigation: PaletteNavigation) {
  const { setActiveIndex, loadMore } = useSearchPaletteStore.getState()
  setActiveIndex(navigation.rows.length - 1)
  void loadMore()
}

function jumpKeys(navigation: PaletteNavigation, event: KeyboardEvent<HTMLInputElement>) {
  if (!hasPrimaryModifier(event)) {
    return {}
  }
  const hasRows = navigation.rows.length > 0
  const endKey: KeyHandlerTable = hasRows ? { End: () => jumpToLastRow(navigation) } : {}
  return { Home: () => useSearchPaletteStore.getState().setActiveIndex(0), ...endKey }
}

function completedQuery(navigation: PaletteNavigation, activeRow: SearchHit) {
  if (navigation.searching) {
    return completeSearchQuery(navigation.query, activeRow)
  }
  return withTrailingSeparator(activeRow.path)
}

function completionKeys(
  navigation: PaletteNavigation,
  event: KeyboardEvent<HTMLInputElement>,
): KeyHandlerTable {
  const activeRow = navigation.activeRow
  if (event.shiftKey) {
    return {}
  }
  if (!activeRow?.isDir) {
    return {}
  }
  const completed = completedQuery(navigation, activeRow)
  return { Tab: () => useSearchPaletteStore.getState().setQuery(completed) }
}

function paletteKeyTable(
  navigation: PaletteNavigation,
  event: KeyboardEvent<HTMLInputElement>,
): KeyHandlerTable {
  const move = (delta: number) => movePaletteSelection(navigation, delta)
  return {
    ArrowDown: () => move(1),
    ArrowUp: () => move(-1),
    PageDown: () => move(SEARCH_PAGE_ROWS),
    PageUp: () => move(-SEARCH_PAGE_ROWS),
    Escape: () => clearQueryOrDismiss(navigation),
    ...jumpKeys(navigation, event),
    ...completionKeys(navigation, event),
  }
}

export function handlePaletteKeyDown(
  navigation: PaletteNavigation,
  event: KeyboardEvent<HTMLInputElement>,
) {
  const how = searchActivationForEvent(event.nativeEvent, navigation.keymap)
  const activeRow = navigation.activeRow
  const activatesRow = how !== null && activeRow !== undefined
  if (activatesRow) {
    event.preventDefault()
    activatePaletteHit(activeRow, how)
    return
  }
  if (isSearchEverywhereShortcut(event.nativeEvent, navigation.keymap)) {
    event.preventDefault()
    navigation.inputRef.current?.select()
    return
  }
  handleKeyByTable(event, paletteKeyTable(navigation, event))
}
