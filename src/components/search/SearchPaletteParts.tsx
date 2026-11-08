import type { CSSProperties, KeyboardEvent, RefObject } from 'react'
import { LoaderCircleIcon, SearchIcon } from '@/components/icons'
import { SearchNoResults, SearchUnavailableState } from '@/components/search/SearchEmptyState'
import { SearchResultRow } from '@/components/search/SearchResultRow'
import { FOCUS_RING_INSET } from '@/lib/focus-ring'
import { formatShortcutLabel, type PlatformOs } from '@/lib/keymap'
import type { SearchActivation } from '@/lib/search-actions'
import type { SearchHit, SearchRejection, SearchScope, Shortcut } from '@/lib/types/ipc'
import type { SearchStatus } from '@/stores/search-palette-store'
import { TEXT_SECONDARY } from '@/lib/class-names'
import type { SearchBackend } from './search-palette-model'

const SEARCH_LISTBOX_ID = 'search-everywhere-results'
const LOAD_MORE_ROW_KEY = 'more'
const SCOPES: { scope: SearchScope; label: string }[] = [
  { scope: 'all', label: 'All' },
  { scope: 'folders', label: 'Folders' },
  { scope: 'files', label: 'Files' },
]
const SELECTED_SCOPE_CLASS =
  'bg-light-card font-semibold text-light-text shadow-thumb dark:bg-dark-card dark:text-dark-text'
const UNSELECTED_SCOPE_CLASS = `${TEXT_SECONDARY} hover:bg-light-hover dark:hover:bg-dark-hover`

function scopeOptionStateClass(isSelected: boolean) {
  return isSelected ? SELECTED_SCOPE_CLASS : UNSELECTED_SCOPE_CLASS
}

export function SearchListMessage({
  status,
  error,
  backend,
  total,
  query,
  foldersOnly,
}: {
  status: SearchStatus
  error: SearchRejection | null
  backend: SearchBackend
  total: number
  query: string
  foldersOnly: boolean
}) {
  const rejection = status === 'error' ? error : null
  if (rejection) {
    return <SearchUnavailableState backend={backend} rejection={rejection} />
  }
  const foundNothing = status === 'ready' && total === 0
  if (foundNothing) {
    return <SearchNoResults query={query} foldersOnly={foldersOnly} />
  }
  return null
}

function SearchInputAdornments({
  shortcut,
  platform,
  loading,
}: {
  shortcut: Shortcut | undefined
  platform: PlatformOs
  loading: boolean
}) {
  return (
    <>
      {shortcut ? (
        <kbd className="shrink-0 rounded-control border border-light-divider px-1.5 font-ui text-caption text-light-text-tertiary dark:border-dark-divider dark:text-dark-text-tertiary">
          {formatShortcutLabel(shortcut, platform)}
        </kbd>
      ) : null}
      {loading ? (
        <span
          aria-hidden="true"
          className="absolute inset-x-0 -bottom-0.5 h-0.5 animate-pulse bg-light-accent dark:bg-dark-accent"
        />
      ) : null}
    </>
  )
}

export function SearchInputField({
  inputRef,
  showList,
  activeOptionId,
  query,
  placeholder,
  onQueryChange,
  onKeyDown,
  shortcut,
  platform,
  loading,
}: {
  inputRef: RefObject<HTMLInputElement | null>
  showList: boolean
  activeOptionId: string | undefined
  query: string
  placeholder: string
  onQueryChange: (query: string) => void
  onKeyDown: (event: KeyboardEvent<HTMLInputElement>) => void
  shortcut: Shortcut | undefined
  platform: PlatformOs
  loading: boolean
}) {
  return (
    <label className="relative mx-2 mt-2 flex h-command items-center gap-2.5 rounded-control border border-light-control-stroke border-b-2 border-b-light-accent bg-light-control px-2.5 focus-within:ring-2 focus-within:ring-light-focus-outer dark:border-dark-control-stroke dark:border-b-dark-accent dark:bg-dark-control dark:focus-within:ring-dark-focus-outer">
      <SearchIcon className={`size-4 shrink-0 ${TEXT_SECONDARY}`} />
      <input
        ref={inputRef}
        role="combobox"
        aria-expanded={showList}
        aria-controls={SEARCH_LISTBOX_ID}
        aria-autocomplete="list"
        aria-activedescendant={activeOptionId}
        aria-label="Search everywhere"
        autoComplete="off"
        spellCheck={false}
        value={query}
        onChange={(event) => onQueryChange(event.target.value)}
        onKeyDown={onKeyDown}
        placeholder={placeholder}
        className="min-w-0 flex-1 select-text bg-transparent text-body outline-none placeholder:text-light-text-tertiary dark:placeholder:text-dark-text-tertiary"
      />
      <SearchInputAdornments shortcut={shortcut} platform={platform} loading={loading} />
    </label>
  )
}

export function SearchScopeBar({
  scope,
  foldersOnly,
  scopeNote,
  onSelectScope,
}: {
  scope: SearchScope
  foldersOnly: boolean
  scopeNote: string
  onSelectScope: (scope: SearchScope) => void
}) {
  return (
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
            onClick={() => onSelectScope(option.scope)}
            className={`h-6 rounded-control px-2.5 text-caption disabled:cursor-not-allowed disabled:text-light-text-disabled dark:disabled:text-dark-text-disabled ${scopeOptionStateClass(scope === option.scope)} ${FOCUS_RING_INSET}`}
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
  )
}

function LoadMoreRow({ style, loadingMore }: { style: CSSProperties; loadingMore: boolean }) {
  return (
    <div
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

export type SearchResultsListProps = {
  rows: SearchHit[]
  virtualRows: Array<{ index: number; start: number }>
  listHeight: number
  searching: boolean
  total: number
  activeIndex: number
  loadingMore: boolean
  onHover: (index: number) => void
  onActivate: (hit: SearchHit, how: SearchActivation) => void
}

export function SearchResultsList(props: SearchResultsListProps) {
  const { rows, searching, activeIndex } = props
  return (
    <div
      role="listbox"
      id={SEARCH_LISTBOX_ID}
      aria-label={searching ? 'Results' : 'Favourites and recent folders'}
      className="relative w-full"
      style={{ height: props.listHeight }}
    >
      {props.virtualRows.map((virtualRow) => {
        const style = { transform: `translateY(${virtualRow.start}px)` }
        const hit = rows[virtualRow.index]
        if (!hit) {
          return (
            <LoadMoreRow key={LOAD_MORE_ROW_KEY} style={style} loadingMore={props.loadingMore} />
          )
        }
        return (
          <SearchResultRow
            key={`${virtualRow.index}:${hit.path}`}
            hit={hit}
            index={virtualRow.index}
            total={searching ? props.total : rows.length}
            active={virtualRow.index === activeIndex}
            style={style}
            onHover={props.onHover}
            onActivate={props.onActivate}
          />
        )
      })}
    </div>
  )
}

export function SearchPaletteFooter({
  footer,
  announcement,
}: {
  footer: string
  announcement: string
}) {
  return (
    <>
      <div className="flex h-8 shrink-0 items-center gap-3.5 overflow-hidden whitespace-nowrap border-t border-light-divider px-3 text-caption text-light-text-tertiary dark:border-dark-divider dark:text-dark-text-tertiary">
        <span className={`truncate ${TEXT_SECONDARY}`}>{footer}</span>
        <span aria-hidden="true" className="ml-auto flex gap-3">
          <span>↵ open</span>
          <span>⇧↵ open file</span>
          <span>Esc close</span>
        </span>
      </div>
      <span role="status" aria-live="polite" className="sr-only">
        {announcement}
      </span>
    </>
  )
}
