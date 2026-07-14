import type { DirectoryEntry } from '@/lib/types/ipc'
import { columnDefinitions } from '@/lib/columns'
import { dateToneClassName, formatEntryDate } from '@/lib/date-format'
import { AlertTriangleIcon } from '@/components/icons'
import { EntryIcon } from '@/components/icons/EntryIcon'
import { useConfigStore } from '@/stores/config-store'
import { useLayoutStore } from '@/stores/layout-store'
import { columnFlexStyle } from './HeaderRow'
import { SizeValue } from './SizeValue'

import type { DragEvent, KeyboardEvent, MouseEvent } from 'react'
import { memo, useEffect, useMemo, useRef } from 'react'

/**
 * Stable, id-keyed handler dispatcher for a pane's rows.
 *
 * `FilePane` builds exactly one of these per pane (memoized so its identity
 * never changes across renders) and every `FileRow` binds `entry.id` itself
 * when calling into it. Keeping this a single stable prop — instead of ~9-13
 * fresh per-row arrow-function props recreated on every `FilePane` render —
 * is what lets `React.memo` below actually skip re-rendering rows whose own
 * props (entry/selection/focus/...) didn't change.
 */
export type FileRowActions = {
  onPointerDown: () => void
  onActivate: (entryId: string, eventTimeStamp: number) => void
  onClick: (entryId: string, event: MouseEvent<HTMLDivElement>) => void
  onContextMenu: (entryId: string, event: MouseEvent<HTMLDivElement>) => void
  onMiddleClick: (entryId: string) => void
  onDragStart: (entryId: string, event: DragEvent<HTMLDivElement>) => void
  onDragEnd: () => void
  onDragEnter: (entryId: string, event: DragEvent<HTMLDivElement>) => void
  onDragOver: (entryId: string, event: DragEvent<HTMLDivElement>) => void
  onDragLeave: (entryId: string) => void
  onDrop: (entryId: string, event: DragEvent<HTMLDivElement>) => void
  onRenameChange: (value: string) => void
  onRenameSubmit: () => void
  onRenameCancel: () => void
  onRenameBlur: () => void
}

type FileRowProps = {
  entry: DirectoryEntry
  isActivePane: boolean
  isFocused: boolean
  isSelected: boolean
  actions: FileRowActions
  isCut?: boolean
  isRenaming?: boolean
  renameValue?: string
  renameBusy?: boolean
  renameError?: string | null
  /** True while a valid internal drag is hovering this (directory) row. */
  isDropTarget?: boolean
  draggable?: boolean
}

function FileRowImpl({
  entry,
  isActivePane,
  isFocused,
  isSelected,
  actions,
  isCut = false,
  isRenaming = false,
  renameValue = '',
  renameBusy = false,
  renameError = null,
  isDropTarget = false,
  draggable = false,
}: FileRowProps) {
  const columns = useLayoutStore((state) => state.columns)
  const columnWidths = useLayoutStore((state) => state.columnWidths)
  const dateFormat = useConfigStore((state) => state.dateFormat)
  const showTime = useConfigStore((state) => state.showTime)
  const showSeconds = useConfigStore((state) => state.showSeconds)
  const relativeDates = useConfigStore((state) => state.relativeDates)
  const visibleColumns = useMemo(() => columns.filter((column) => column.visible), [columns])
  const renameInputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    if (!isRenaming) {
      return
    }

    renameInputRef.current?.focus()
    renameInputRef.current?.select()
  }, [isRenaming])

  const rowClassName = `group flex h-row w-full items-center gap-3 border-b border-light-border px-3 text-row text-left focus-visible:outline-none dark:border-dark-border ${
    isSelected ? 'bg-accent-blue-soft' : 'bg-light-surface dark:bg-dark-surface'
  } ${isFocused && isActivePane ? 'ring-2 ring-inset ring-accent-blue-border' : ''} ${
    isCut ? 'opacity-50' : ''
  } hover:bg-light-hover dark:hover:bg-dark-hover`

  function onRenameKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    event.stopPropagation()
    if (event.key === 'Enter') {
      event.preventDefault()
      actions.onRenameSubmit()
    } else if (event.key === 'Escape') {
      event.preventDefault()
      actions.onRenameCancel()
    }
  }

  if (isRenaming) {
    return (
      <div role="row" data-entry-id={entry.id} className={rowClassName}>
        {visibleColumns.map((column) => {
          const definition = columnDefinitions[column.key]

          if (column.key === 'name') {
            return (
              <span
                key={column.key}
                style={columnFlexStyle(column.key, columnWidths)}
                className={definition.className}
              >
                <span className="flex min-w-0 items-center gap-2">
                  <EntryIcon entry={entry} />
                  <input
                    ref={renameInputRef}
                    aria-label={`Rename ${entry.name}`}
                    value={renameValue}
                    disabled={renameBusy}
                    onMouseDown={(event) => event.stopPropagation()}
                    onBlur={actions.onRenameBlur}
                    onChange={(event) => actions.onRenameChange(event.target.value)}
                    onKeyDown={onRenameKeyDown}
                    className="min-w-0 flex-1 select-text rounded-tab border border-accent-blue-border bg-light-window px-2 py-1 font-mono text-uxs text-light-text outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:bg-dark-window dark:text-dark-text"
                  />
                </span>
              </span>
            )
          }

          if (column.key === 'size') {
            return (
              <span
                key={column.key}
                style={columnFlexStyle(column.key, columnWidths)}
                className={definition.className}
              >
                <span className="font-mono text-uxs text-light-text-soft dark:text-dark-text-soft">
                  <SizeValue entry={entry} />
                </span>
              </span>
            )
          }

          if (column.key === 'items') {
            return (
              <span
                key={column.key}
                style={columnFlexStyle(column.key, columnWidths)}
                className={definition.className}
              >
                <span className="font-mono text-uxs text-light-text-muted dark:text-dark-text-muted">
                  {entry.isDir ? (entry.itemCount ?? '—') : '—'}
                </span>
              </span>
            )
          }

          if (column.key === 'type') {
            return (
              <span
                key={column.key}
                style={columnFlexStyle(column.key, columnWidths)}
                className={definition.className}
              >
                {renameError ? (
                  <span className="flex items-center gap-1 text-uxs text-accent-amber">
                    <AlertTriangleIcon className="h-3.5 w-3.5 shrink-0" />
                    <span className="truncate">{renameError}</span>
                  </span>
                ) : (
                  <span className="truncate text-usm text-light-text-muted dark:text-dark-text-muted">
                    {renameBusy ? 'Renaming…' : 'Rename'}
                  </span>
                )}
              </span>
            )
          }

          const formatted = formatEntryDate(
            column.key === 'created' ? entry.createdAt : entry.modifiedAt,
            { format: dateFormat, showTime, showSeconds, relative: relativeDates },
          )
          return (
            <span
              key={column.key}
              style={columnFlexStyle(column.key, columnWidths)}
              className={definition.className}
            >
              <span className={`font-mono text-uxs ${dateToneClassName[formatted.tone]}`}>
                {formatted.text}
              </span>
            </span>
          )
        })}
      </div>
    )
  }

  return (
    <div
      role="row"
      data-entry-id={entry.id}
      draggable={draggable}
      onDragStart={(event) => actions.onDragStart(entry.id, event)}
      onDragEnd={actions.onDragEnd}
      onDragEnter={(event) => actions.onDragEnter(entry.id, event)}
      onDragOver={(event) => actions.onDragOver(entry.id, event)}
      onDragLeave={() => actions.onDragLeave(entry.id)}
      onDrop={(event) => actions.onDrop(entry.id, event)}
      // A non-focusable div (not a <button>) so mousedown never steals keyboard
      // focus away from the pane <section>; that lets us skip the mousedown
      // preventDefault a <button> needed, which would otherwise suppress the
      // native `dragstart` and break drag-and-drop entirely.
      onMouseDown={actions.onPointerDown}
      onDoubleClick={(event) => actions.onActivate(entry.id, event.timeStamp)}
      onClick={(event) => actions.onClick(entry.id, event)}
      onContextMenu={(event) => actions.onContextMenu(entry.id, event)}
      onAuxClick={(event) => {
        if (event.button === 1 && entry.isDir && !entry.trashId) {
          event.preventDefault()
          actions.onMiddleClick(entry.id)
        }
      }}
      className={`${rowClassName} cursor-pointer select-none ${
        isDropTarget ? 'ring-2 ring-inset ring-accent-blue-border bg-accent-blue-soft' : ''
      }`}
    >
      {visibleColumns.map((column) => {
        const definition = columnDefinitions[column.key]

        if (column.key === 'name') {
          return (
            <span
              key={column.key}
              style={columnFlexStyle(column.key, columnWidths)}
              className={definition.className}
            >
              <span className="flex min-w-0 items-center gap-2">
                <EntryIcon entry={entry} />
                <span
                  title={
                    entry.originalPath ? `Original location: ${entry.originalPath}` : undefined
                  }
                  className={`truncate ${
                    isCut
                      ? 'text-light-text-soft dark:text-dark-text-soft'
                      : 'text-light-text dark:text-dark-text'
                  }`}
                >
                  {entry.name}
                </span>
              </span>
            </span>
          )
        }

        if (column.key === 'size') {
          return (
            <span
              key={column.key}
              style={columnFlexStyle(column.key, columnWidths)}
              className={definition.className}
            >
              <span className="font-mono text-uxs text-light-text-soft dark:text-dark-text-soft">
                <SizeValue entry={entry} />
              </span>
            </span>
          )
        }

        if (column.key === 'items') {
          return (
            <span
              key={column.key}
              style={columnFlexStyle(column.key, columnWidths)}
              className={definition.className}
            >
              <span className="font-mono text-uxs text-light-text-muted dark:text-dark-text-muted">
                {entry.isDir ? (entry.itemCount ?? '—') : '—'}
              </span>
            </span>
          )
        }

        if (column.key === 'type') {
          return (
            <span
              key={column.key}
              style={columnFlexStyle(column.key, columnWidths)}
              className={definition.className}
            >
              <span className="truncate text-usm text-light-text-muted dark:text-dark-text-muted">
                {entry.typeLabel}
              </span>
            </span>
          )
        }

        const formatted = formatEntryDate(
          column.key === 'created' ? entry.createdAt : entry.modifiedAt,
          { format: dateFormat, showTime, showSeconds, relative: relativeDates },
        )
        return (
          <span
            key={column.key}
            style={columnFlexStyle(column.key, columnWidths)}
            className={definition.className}
          >
            <span className={`font-mono text-uxs ${dateToneClassName[formatted.tone]}`}>
              {formatted.text}
            </span>
          </span>
        )
      })}
    </div>
  )
}

/**
 * Memoized so a pane holding hundreds/thousands of rows only re-renders the
 * rows whose own props actually changed (e.g. the previous and next focused
 * row on an arrow-key move) rather than every visible row on every keystroke.
 * This only helps because `actions` (see `FileRowActions` above) and
 * `isSelected` (a `Set`-backed membership check in `FilePane`) are stable /
 * cheap to recompute — see `FilePane.tsx`.
 */
export const FileRow = memo(FileRowImpl)
