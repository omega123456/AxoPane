import type { CSSProperties, MouseEvent } from 'react'
import { EntryIcon } from '@/components/icons/EntryIcon'
import { HighlightedText } from '@/components/search/HighlightedText'
import { formatEntryDate } from '@/lib/date-format'
import type { SearchActivation } from '@/lib/search-actions'
import type { SearchHit } from '@/lib/types/ipc'
import { useDateDisplay } from '@/lib/use-date-display'
import { HOVER_ON_POINTER, TEXT_SECONDARY } from '@/lib/class-names'

const ACTIVE_RESULT_ROW_CLASS =
  'bg-light-neutral-selection before:absolute before:inset-y-3 before:left-0 before:w-0.75 before:rounded-full before:bg-light-accent dark:bg-dark-neutral-selection dark:before:bg-dark-accent'

function resultRowStateClass(active: boolean) {
  return active ? ACTIVE_RESULT_ROW_CLASS : HOVER_ON_POINTER
}

export function searchOptionId(index: number) {
  return `search-option-${index}`
}

function activationForClick(event: MouseEvent): SearchActivation {
  if (event.shiftKey) {
    return 'open'
  }
  if (event.ctrlKey || event.metaKey) {
    return 'newTab'
  }
  return 'default'
}

export function SearchResultRow({
  hit,
  index,
  total,
  active,
  style,
  onHover,
  onActivate,
}: {
  hit: SearchHit
  index: number
  total: number
  active: boolean
  style: CSSProperties
  onHover: (index: number) => void
  onActivate: (hit: SearchHit, how: SearchActivation) => void
}) {
  const modified = formatEntryDate(hit.modifiedAt, useDateDisplay())

  return (
    <div
      role="option"
      id={searchOptionId(index)}
      aria-selected={active}
      aria-setsize={total}
      aria-posinset={index + 1}
      title={hit.path}
      style={style}
      onMouseMove={() => onHover(index)}
      // Keep focus in the input: the row is chosen through the combobox.
      onMouseDown={(event) => event.preventDefault()}
      onClick={(event) => onActivate(hit, activationForClick(event))}
      className={`absolute inset-x-0 top-0 flex h-search-row cursor-default items-center gap-2.5 rounded-control px-2.5 ${resultRowStateClass(active)}`}
    >
      <span className="flex size-entry-icon shrink-0 items-center justify-center">
        <EntryIcon entry={{ name: hit.name, isDir: hit.isDir }} className="size-5 shrink-0" />
      </span>
      <span className="grid min-w-0 flex-1">
        <span className="truncate text-body text-light-text dark:text-dark-text">
          <HighlightedText text={hit.name} ranges={hit.nameRanges} />
        </span>
        <span className={`truncate text-caption ${TEXT_SECONDARY}`}>
          <HighlightedText text={hit.parentPath} ranges={hit.parentRanges} />
        </span>
      </span>
      {hit.modifiedAt ? (
        <span className="shrink-0 text-caption tabular-nums text-light-text-tertiary dark:text-dark-text-tertiary">
          {modified.text}
        </span>
      ) : null}
    </div>
  )
}
