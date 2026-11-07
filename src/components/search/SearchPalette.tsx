import { useEffect, useMemo, useRef, type KeyboardEvent } from 'react'
import { LoaderCircleIcon, SearchIcon } from '@/components/icons'
import { SearchNoResults, SearchUnavailableState } from '@/components/search/SearchEmptyState'
import { SearchResultRow, searchOptionId } from '@/components/search/SearchResultRow'
import { favouriteLabel } from '@/components/tree/FavouriteTreeRow'
import { FOCUS_RING_INSET } from '@/lib/focus-ring'
import { compactCount, folderSizeStatusLabel } from '@/lib/folder-size-status'
import { detectPlatformOs, formatShortcutLabel, resolveCommandForEvent } from '@/lib/keymap'
import {
  activateSearchHit,
  completeSearchQuery,
  searchActivationForEvent,
  type SearchActivation,
} from '@/lib/search-actions'
import type { FolderSizeIndexStatus, SearchHit, SearchScope } from '@/lib/types/ipc'
import { useElementVirtualizer } from '@/lib/use-element-virtualizer'
import { useConfigStore } from '@/stores/config-store'
import { useKeymapStore } from '@/stores/keymap-store'
import { getParentPath, usePanesStore } from '@/stores/panes-store'
import { useRecentFoldersStore } from '@/stores/recent-folders-store'
import { isSearchable, useSearchPaletteStore } from '@/stores/search-palette-store'
import { TEXT_SECONDARY } from '@/lib/class-names'

const ROW_HEIGHT = 44
const PAGE_ROWS = 8
const UNMEASURED_FALLBACK_ROWS = PAGE_ROWS + PAGE_ROWS
const MAX_RECENTS = 6
const LISTBOX_ID = 'search-everywhere-results'
const SCOPES: { scope: SearchScope; label: string }[] = [
  { scope: 'all', label: 'All' },
  { scope: 'folders', label: 'Folders' },
  { scope: 'files', label: 'Files' },
]

/** A favourite or recent folder, shaped like a hit so rows and actions are shared. */
function folderRow(path: string, name: string): SearchHit {
  return {
    path,
    name,
    parentPath: getParentPath(path) ?? '',
    isDir: true,
    modifiedAt: null,
    nameRanges: [],
    parentRanges: [],
  }
}

function withTrailingSeparator(path: string) {
  return /[\\/]$/.test(path) ? path : `${path}${path.includes('\\') ? '\\' : '/'}`
}

function footerText(
  backend: 'everything' | 'macIndex' | 'none',
  status: FolderSizeIndexStatus | null,
  searching: boolean,
  loaded: number,
  total: number,
  truncated: boolean,
) {
  const count =
    loaded < total
      ? `Showing ${loaded} of ${total}`
      : `${total} ${total === 1 ? 'result' : 'results'}`
  const results = searching ? ` · ${count}${truncated ? ' · refine your search' : ''}` : ''
  if (backend !== 'macIndex') {
    return `Everything${results}`
  }
  if (status?.state === 'indexing') {
    const scanned =
      status.itemsScanned === undefined
        ? ''
        : ` ${compactCount.format(status.itemsScanned)} folders`
    return `Indexing…${scanned} · results may be incomplete`
  }
  if (status?.state === 'paused') {
    return folderSizeStatusLabel(status)
  }
  return `Folders on this Mac${results}`
}

export function SearchPalette() {
  const isOpen = useSearchPaletteStore((state) => state.isOpen)
  return isOpen ? <SearchPaletteFlyout /> : null
}

function SearchPaletteFlyout() {
  const query = useSearchPaletteStore((state) => state.query)
  const scope = useSearchPaletteStore((state) => state.scope)
  const status = useSearchPaletteStore((state) => state.status)
  const items = useSearchPaletteStore((state) => state.items)
  const total = useSearchPaletteStore((state) => state.total)
  const truncated = useSearchPaletteStore((state) => state.truncated)
  const loadingMore = useSearchPaletteStore((state) => state.loadingMore)
  const activeIndex = useSearchPaletteStore((state) => state.activeIndex)
  const error = useSearchPaletteStore((state) => state.error)
  const capabilities = useSearchPaletteStore((state) => state.capabilities)
  const focusRequest = useSearchPaletteStore((state) => state.focusRequest)
  const { close, setQuery, setScope, setActiveIndex, moveActive, loadMore } =
    useSearchPaletteStore.getState()
  const indexStatus = usePanesStore((state) => state.folderSizeIndexStatus)
  const favourites = useConfigStore((state) => state.favourites)
  const favouriteNames = useConfigStore((state) => state.favouriteNames)
  const recentFolders = useRecentFoldersStore((state) => state.folders)
  const keymap = useKeymapStore((state) => state.bindings)
  const inputRef = useRef<HTMLInputElement>(null)
  const panelRef = useRef<HTMLDivElement>(null)
  const listRef = useRef<HTMLDivElement>(null)

  const backend = capabilities?.backend ?? indexStatus?.backend ?? 'none'
  const foldersOnly = capabilities?.foldersOnly ?? backend === 'macIndex'
  const searching = status !== 'idle'
  const shortcut = keymap.searchEverywhere[0]
  const platform = detectPlatformOs()

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
  const rows = searching ? items : [...favouriteRows, ...recentRows]
  const hasMore = searching && items.length < total
  const activeRow = rows[activeIndex]

  const rowCount = rows.length + (hasMore ? 1 : 0)
  const virtualizer = useElementVirtualizer({
    count: rowCount,
    estimateSize: () => ROW_HEIGHT,
    getScrollElement: () => listRef.current,
    overscan: PAGE_ROWS,
  })
  const measuredRows = virtualizer.getVirtualItems()
  // Before the list is measured, render a bounded first screen (as the
  // details view does) instead of an empty frame.
  const virtualRows =
    measuredRows.length > 0 || rowCount === 0
      ? measuredRows
      : Array.from({ length: Math.min(rowCount, UNMEASURED_FALLBACK_ROWS) }, (_, index) => ({
          index,
          start: index * ROW_HEIGHT,
        }))
  const lastRendered = virtualRows.at(-1)?.index ?? -1

  useEffect(() => {
    inputRef.current?.focus()
    inputRef.current?.select()
  }, [focusRequest])

  useEffect(() => {
    function onPointerDown(event: PointerEvent) {
      const target = event.target as Element
      if (!panelRef.current?.contains(target) && !target.closest('[data-search-trigger]')) {
        close()
      }
    }
    document.addEventListener('pointerdown', onPointerDown)
    return () => document.removeEventListener('pointerdown', onPointerDown)
  }, [close])

  useEffect(() => {
    if (rows.length > 0) {
      virtualizer.scrollToIndex(activeIndex, { align: 'auto' })
    }
  }, [activeIndex, rows.length, virtualizer])

  // Scrolling the loading row into view fetches the next page.
  useEffect(() => {
    if (hasMore && lastRendered >= items.length) {
      void loadMore()
    }
  }, [hasMore, items.length, lastRendered, loadMore])

  function dismiss() {
    const paneId = usePanesStore.getState().activePaneId
    close()
    document.querySelector<HTMLElement>(`[data-pane-id="${paneId}"]`)?.focus()
  }

  function activate(hit: SearchHit, how: SearchActivation) {
    dismiss()
    void activateSearchHit(hit, how)
  }

  function move(delta: number) {
    if (searching) {
      moveActive(delta)
    } else if (rows.length > 0) {
      setActiveIndex(Math.min(Math.max(activeIndex + delta, 0), rows.length - 1))
    }
  }

  function onKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    const how = searchActivationForEvent(event.nativeEvent, keymap)
    if (how && activeRow) {
      event.preventDefault()
      activate(activeRow, how)
      return
    }
    const command = resolveCommandForEvent(event.nativeEvent, keymap)
    if (command === 'searchEverywhere') {
      event.preventDefault()
      inputRef.current?.select()
      return
    }
    const ctrl = event.ctrlKey || event.metaKey
    switch (event.key) {
      case 'ArrowDown':
        event.preventDefault()
        move(1)
        break
      case 'ArrowUp':
        event.preventDefault()
        move(-1)
        break
      case 'PageDown':
        event.preventDefault()
        move(PAGE_ROWS)
        break
      case 'PageUp':
        event.preventDefault()
        move(-PAGE_ROWS)
        break
      case 'Home':
        if (ctrl) {
          event.preventDefault()
          setActiveIndex(0)
        }
        break
      case 'End':
        if (ctrl && rows.length > 0) {
          event.preventDefault()
          setActiveIndex(rows.length - 1)
          void loadMore()
        }
        break
      case 'Tab':
        if (!event.shiftKey && activeRow?.isDir) {
          event.preventDefault()
          setQuery(
            searching
              ? completeSearchQuery(query, activeRow)
              : withTrailingSeparator(activeRow.path),
          )
        }
        break
      case 'Escape':
        event.preventDefault()
        if (query) {
          setQuery('')
        } else {
          dismiss()
        }
        break
    }
  }

  const shortQuery = !searching && query.trim().length > 0 && !isSearchable(query)
  const scopeNote = shortQuery
    ? 'Keep typing. Search starts at 3 characters'
    : foldersOnly
      ? 'Folder names only on this Mac'
      : ''
  const announcement = status === 'ready' ? (total === 0 ? 'No results' : `${total} results`) : ''
  const showList = rows.length > 0 && status !== 'error'

  return (
    <div className="pointer-events-none fixed inset-x-0 top-palette-top z-40 flex justify-center px-4">
      <div
        ref={panelRef}
        role="dialog"
        aria-label="Search everywhere"
        className="pointer-events-auto flex w-palette max-w-full flex-col overflow-hidden rounded-overlay border border-light-flyout-stroke bg-light-card text-light-text shadow-light-flyout dark:border-dark-flyout-stroke dark:bg-dark-card dark:text-dark-text dark:shadow-dark-flyout"
      >
        <label className="relative mx-2 mt-2 flex h-command items-center gap-2.5 rounded-control border border-light-control-stroke border-b-2 border-b-light-accent bg-light-control px-2.5 focus-within:ring-2 focus-within:ring-light-focus-outer dark:border-dark-control-stroke dark:border-b-dark-accent dark:bg-dark-control dark:focus-within:ring-dark-focus-outer">
          <SearchIcon className={`size-4 shrink-0 ${TEXT_SECONDARY}`} />
          <input
            ref={inputRef}
            role="combobox"
            aria-expanded={showList}
            aria-controls={LISTBOX_ID}
            aria-autocomplete="list"
            aria-activedescendant={activeRow ? searchOptionId(activeIndex) : undefined}
            aria-label="Search everywhere"
            autoComplete="off"
            spellCheck={false}
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={onKeyDown}
            placeholder={
              foldersOnly
                ? 'Search folders, or type a path like alex/'
                : 'Search files and folders, or type a path like alex/'
            }
            className="min-w-0 flex-1 select-text bg-transparent text-body outline-none placeholder:text-light-text-tertiary dark:placeholder:text-dark-text-tertiary"
          />
          {shortcut ? (
            <kbd className="shrink-0 rounded-control border border-light-divider px-1.5 font-ui text-caption text-light-text-tertiary dark:border-dark-divider dark:text-dark-text-tertiary">
              {formatShortcutLabel(shortcut, platform)}
            </kbd>
          ) : null}
          {status === 'loading' ? (
            <span
              aria-hidden="true"
              className="absolute inset-x-0 -bottom-0.5 h-0.5 animate-pulse bg-light-accent dark:bg-dark-accent"
            />
          ) : null}
        </label>

        <div className="flex flex-wrap items-center gap-2.5 px-2.5 pb-1.5 pt-2">
          <div
            role="group"
            aria-label="Search scope"
            className="inline-flex gap-0.5 rounded-control border border-light-stroke bg-light-base p-0.5 dark:border-dark-stroke dark:bg-dark-base"
          >
            {SCOPES.map((option) => (
              <button
                key={option.scope}
                type="button"
                aria-pressed={scope === option.scope}
                disabled={option.scope === 'files' && foldersOnly}
                onClick={() => {
                  setScope(option.scope)
                  inputRef.current?.focus()
                }}
                className={`h-6 rounded-control px-2.5 text-caption disabled:cursor-not-allowed disabled:text-light-text-disabled dark:disabled:text-dark-text-disabled ${
                  scope === option.scope
                    ? 'bg-light-card font-semibold text-light-text shadow-thumb dark:bg-dark-card dark:text-dark-text'
                    : `${TEXT_SECONDARY} hover:bg-light-hover dark:hover:bg-dark-hover`
                } ${FOCUS_RING_INSET}`}
              >
                {option.label}
              </button>
            ))}
          </div>
          {scopeNote ? (
            <span className="min-w-0 flex-1 truncate text-caption text-light-text-tertiary dark:text-dark-text-tertiary">
              {scopeNote}
            </span>
          ) : null}
        </div>

        <div ref={listRef} className="max-h-palette-list min-h-0 overflow-y-auto px-1 pb-1">
          {status === 'error' && error ? (
            <SearchUnavailableState backend={backend} rejection={error} />
          ) : status === 'ready' && total === 0 ? (
            <SearchNoResults query={query} foldersOnly={foldersOnly} />
          ) : null}
          {showList ? (
            <div
              role="listbox"
              id={LISTBOX_ID}
              aria-label={searching ? 'Results' : 'Favourites and recent folders'}
              className="relative w-full"
              style={{ height: virtualizer.getTotalSize() || rowCount * ROW_HEIGHT }}
            >
              {virtualRows.map((virtualRow) => {
                const style = { transform: `translateY(${virtualRow.start}px)` }
                const hit = rows[virtualRow.index]
                if (!hit) {
                  return (
                    <div
                      key="more"
                      aria-hidden="true"
                      style={style}
                      className="absolute inset-x-0 top-0 flex h-search-row items-center justify-center gap-2 text-caption text-light-text-tertiary dark:text-dark-text-tertiary"
                    >
                      {loadingMore ? (
                        <>
                          <LoaderCircleIcon className="size-3.5 animate-spin" />
                          Loading more…
                        </>
                      ) : (
                        'Scroll or press ↓ for more'
                      )}
                    </div>
                  )
                }
                return (
                  <SearchResultRow
                    key={`${virtualRow.index}:${hit.path}`}
                    hit={hit}
                    index={virtualRow.index}
                    total={searching ? total : rows.length}
                    active={virtualRow.index === activeIndex}
                    style={style}
                    onHover={setActiveIndex}
                    onActivate={activate}
                  />
                )
              })}
            </div>
          ) : null}
        </div>

        <div className="flex h-8 shrink-0 items-center gap-3.5 overflow-hidden whitespace-nowrap border-t border-light-divider px-3 text-caption text-light-text-tertiary dark:border-dark-divider dark:text-dark-text-tertiary">
          <span className={`truncate ${TEXT_SECONDARY}`}>
            {footerText(backend, indexStatus, status === 'ready', items.length, total, truncated)}
          </span>
          <span aria-hidden="true" className="ml-auto flex gap-3">
            <span>↵ open</span>
            <span>⇧↵ open file</span>
            <span>Esc close</span>
          </span>
        </div>
        <span role="status" aria-live="polite" className="sr-only">
          {announcement}
        </span>
      </div>
    </div>
  )
}
