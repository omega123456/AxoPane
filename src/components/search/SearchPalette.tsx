import { useEffect, useRef, type ReactNode, type RefObject } from 'react'
import { searchOptionId } from '@/components/search/SearchResultRow'
import { detectPlatformOs } from '@/lib/keymap'
import type { SearchScope } from '@/lib/types/ipc'
import { useSearchPaletteStore } from '@/stores/search-palette-store'
import {
  footerText,
  inputPlaceholder,
  lastRenderedIndex,
  paletteRows,
  resultAnnouncement,
  scopeNoteFor,
  searchBackendInfo,
} from './search-palette-model'
import {
  SearchInputField,
  SearchListMessage,
  SearchPaletteFooter,
  SearchResultsList,
  SearchScopeBar,
} from './SearchPaletteParts'
import {
  activatePaletteHit,
  handlePaletteKeyDown,
  SEARCH_ROW_HEIGHT,
  useCloseOnOutsidePointer,
  useFolderShortcutRows,
  useSearchPaletteView,
  useSearchVirtualRows,
} from './use-search-palette'

type SearchPaletteView = ReturnType<typeof useSearchPaletteView>
type SearchVirtualizer = ReturnType<typeof useSearchVirtualRows>['virtualizer']

export function SearchPalette() {
  const isOpen = useSearchPaletteStore((state) => state.isOpen)
  return isOpen ? <SearchPaletteFlyout /> : null
}

function usePaletteRows(view: SearchPaletteView) {
  const { favouriteRows, recentRows } = useFolderShortcutRows()
  const searching = view.status !== 'idle'
  const rows = paletteRows(searching, view.items, favouriteRows, recentRows)
  const hasMore = searching && view.items.length < view.total
  return { searching, rows, hasMore, activeRow: rows[view.activeIndex] }
}

function useFocusInputOnRequest(inputRef: RefObject<HTMLInputElement | null>, request: number) {
  useEffect(() => {
    inputRef.current?.focus()
    inputRef.current?.select()
  }, [inputRef, request])
}

function useLoadMoreWhenLoadingRowRendered(
  hasMore: boolean,
  loadedCount: number,
  lastRendered: number,
) {
  useEffect(() => {
    if (hasMore && lastRendered >= loadedCount) {
      void useSearchPaletteStore.getState().loadMore()
    }
  }, [hasMore, loadedCount, lastRendered])
}

function SearchPaletteFrame({
  panelRef,
  children,
}: {
  panelRef: RefObject<HTMLDivElement | null>
  children: ReactNode
}) {
  return (
    <div className="pointer-events-none fixed inset-x-0 top-palette-top z-40 flex justify-center px-4">
      <div
        ref={panelRef}
        role="dialog"
        aria-label="Search everywhere"
        className="pointer-events-auto flex w-palette max-w-full flex-col overflow-hidden rounded-overlay border border-light-flyout-stroke bg-light-card text-light-text shadow-light-flyout dark:border-dark-flyout-stroke dark:bg-dark-card dark:text-dark-text dark:shadow-dark-flyout"
      >
        {children}
      </div>
    </div>
  )
}

function useScrollActiveRowIntoView(
  virtualizer: SearchVirtualizer,
  activeIndex: number,
  rowCount: number,
) {
  useEffect(() => {
    if (rowCount > 0) virtualizer.scrollToIndex(activeIndex, { align: 'auto' })
  }, [activeIndex, rowCount, virtualizer])
}

function useSearchPaletteController() {
  const view = useSearchPaletteView()
  const inputRef = useRef<HTMLInputElement>(null)
  const panelRef = useRef<HTMLDivElement>(null)
  const listRef = useRef<HTMLDivElement>(null)
  const { searching, rows, hasMore, activeRow } = usePaletteRows(view)
  const rowCount = rows.length + (hasMore ? 1 : 0)
  const { virtualizer, virtualRows } = useSearchVirtualRows(rowCount, listRef)
  useFocusInputOnRequest(inputRef, view.focusRequest)
  useCloseOnOutsidePointer(panelRef, useSearchPaletteStore.getState().close)
  useScrollActiveRowIntoView(virtualizer, view.activeIndex, rows.length)
  useLoadMoreWhenLoadingRowRendered(hasMore, view.items.length, lastRenderedIndex(virtualRows))
  return {
    view,
    refs: { inputRef, panelRef, listRef },
    navigation: { ...view, searching, rows, activeRow, inputRef },
    backendInfo: searchBackendInfo(view.capabilities, view.indexStatus),
    results: {
      rows,
      searching,
      showList: rows.length > 0 && view.status !== 'error',
      virtualRows,
      listHeight: virtualizer.getTotalSize() || rowCount * SEARCH_ROW_HEIGHT,
    },
  }
}

type SearchPaletteController = ReturnType<typeof useSearchPaletteController>

function footerFor({ view, backendInfo }: SearchPaletteController) {
  return footerText({
    backend: backendInfo.backend,
    status: view.indexStatus,
    searching: view.status === 'ready',
    loaded: view.items.length,
    total: view.total,
    truncated: view.truncated,
  })
}

type SearchPaletteResultsProps = {
  listRef: RefObject<HTMLDivElement | null>
  view: SearchPaletteView
  backendInfo: SearchPaletteController['backendInfo']
  results: SearchPaletteController['results']
}

function SearchPaletteResults({ listRef, view, backendInfo, results }: SearchPaletteResultsProps) {
  return (
    <div ref={listRef} className="max-h-palette-list min-h-0 overflow-y-auto px-1 pb-1">
      <SearchListMessage {...view} {...backendInfo} />
      {results.showList ? (
        <SearchResultsList
          {...view}
          {...results}
          onHover={useSearchPaletteStore.getState().setActiveIndex}
          onActivate={activatePaletteHit}
        />
      ) : null}
    </div>
  )
}

function SearchPaletteFlyout() {
  const controller = useSearchPaletteController()
  const { view, navigation, refs } = controller
  const { foldersOnly } = controller.backendInfo
  const { setQuery, setScope } = useSearchPaletteStore.getState()
  const selectScope = (scope: SearchScope) => {
    setScope(scope)
    refs.inputRef.current?.focus()
  }

  return (
    <SearchPaletteFrame panelRef={refs.panelRef}>
      <SearchInputField
        inputRef={refs.inputRef}
        showList={controller.results.showList}
        activeOptionId={navigation.activeRow ? searchOptionId(view.activeIndex) : undefined}
        query={view.query}
        placeholder={inputPlaceholder(foldersOnly)}
        onQueryChange={setQuery}
        onKeyDown={(event) => handlePaletteKeyDown(navigation, event)}
        shortcut={view.keymap.searchEverywhere[0]}
        platform={detectPlatformOs()}
        loading={view.status === 'loading'}
      />
      <SearchScopeBar
        scope={view.scope}
        foldersOnly={foldersOnly}
        scopeNote={scopeNoteFor(navigation.searching, view.query, foldersOnly)}
        onSelectScope={selectScope}
      />
      <SearchPaletteResults
        listRef={refs.listRef}
        view={view}
        backendInfo={controller.backendInfo}
        results={controller.results}
      />
      <SearchPaletteFooter
        footer={footerFor(controller)}
        announcement={resultAnnouncement(view.status, view.total)}
      />
    </SearchPaletteFrame>
  )
}
