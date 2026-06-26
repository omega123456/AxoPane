import type { DirectoryEntry } from '@/lib/types/ipc'
import { columnDefinitions } from '@/lib/columns'
import { AlertTriangleIcon } from '@/components/icons'
import { EntryIcon } from '@/components/icons/EntryIcon'
import { useLayoutStore } from '@/stores/layout-store'
import { SizeValue } from './SizeValue'

type FileRowProps = {
  entry: DirectoryEntry
  isActivePane: boolean
  isFocused: boolean
  isSelected: boolean
  onPointerDown: () => void
  onActivate: () => void
  onClick: (event: MouseEvent<HTMLButtonElement>) => void
  onContextMenu: (event: MouseEvent<HTMLButtonElement>) => void
  onMiddleClick: () => void
  isRenaming?: boolean
  renameValue?: string
  renameBusy?: boolean
  renameError?: string | null
  onRenameChange?: (value: string) => void
  onRenameSubmit?: () => void
  onRenameCancel?: () => void
  onRenameBlur?: () => void
}

import type { KeyboardEvent, MouseEvent } from 'react'
import { useEffect, useMemo, useRef } from 'react'

export function FileRow({
  entry,
  isActivePane,
  isFocused,
  isSelected,
  onPointerDown,
  onActivate,
  onClick,
  onContextMenu,
  onMiddleClick,
  isRenaming = false,
  renameValue = '',
  renameBusy = false,
  renameError = null,
  onRenameChange,
  onRenameSubmit,
  onRenameCancel,
  onRenameBlur,
}: FileRowProps) {
  const columns = useLayoutStore((state) => state.columns)
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
  } ${isFocused && isActivePane ? 'ring-2 ring-inset ring-accent-blue-border' : ''} hover:bg-light-hover dark:hover:bg-dark-hover`

  function onRenameKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    event.stopPropagation()
    if (event.key === 'Enter') {
      event.preventDefault()
      onRenameSubmit?.()
    } else if (event.key === 'Escape') {
      event.preventDefault()
      onRenameCancel?.()
    }
  }

  if (isRenaming) {
    return (
      <div
        role="row"
        data-entry-id={entry.id}
        className={rowClassName}
      >
        {visibleColumns.map((column) => {
          const definition = columnDefinitions[column.key]

          if (column.key === 'name') {
            return (
              <span key={column.key} className={definition.className}>
                <span className="flex min-w-0 items-center gap-2">
                  <EntryIcon entry={entry} />
                  <input
                    ref={renameInputRef}
                    aria-label={`Rename ${entry.name}`}
                    value={renameValue}
                    disabled={renameBusy}
                    onMouseDown={(event) => event.stopPropagation()}
                    onBlur={onRenameBlur}
                    onChange={(event) => onRenameChange?.(event.target.value)}
                    onKeyDown={onRenameKeyDown}
                    className="min-w-0 flex-1 rounded-tab border border-accent-blue-border bg-light-window px-2 py-1 font-mono text-uxs text-light-text outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:bg-dark-window dark:text-dark-text"
                  />
                </span>
              </span>
            )
          }

          if (column.key === 'size') {
            return (
              <span key={column.key} className={definition.className}>
                <span className="font-mono text-uxs text-light-text-soft dark:text-dark-text-soft">
                  <SizeValue entry={entry} />
                </span>
              </span>
            )
          }

          if (column.key === 'items') {
            return (
              <span key={column.key} className={definition.className}>
                <span className="font-mono text-uxs text-light-text-muted dark:text-dark-text-muted">
                  {entry.isDir ? (entry.itemCount ?? '—') : '—'}
                </span>
              </span>
            )
          }

          if (column.key === 'type') {
            return (
              <span key={column.key} className={definition.className}>
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

          const value = column.key === 'created' ? formatDate(entry.createdAt) : formatDate(entry.modifiedAt)
          return (
            <span key={column.key} className={definition.className}>
              <span className="font-mono text-uxs text-light-text-muted dark:text-dark-text-muted">{value}</span>
            </span>
          )
        })}
      </div>
    )
  }

  return (
    <button
      type="button"
      role="row"
      data-entry-id={entry.id}
      onMouseDown={(event) => {
        event.preventDefault()
        onPointerDown()
      }}
      onDoubleClick={onActivate}
      onClick={onClick}
      onContextMenu={onContextMenu}
      onAuxClick={(event) => {
        if (event.button === 1 && entry.isDir) {
          event.preventDefault()
          onMiddleClick()
        }
      }}
      className={rowClassName}
    >
      {visibleColumns.map((column) => {
        const definition = columnDefinitions[column.key]

        if (column.key === 'name') {
          return (
            <span key={column.key} className={definition.className}>
              <span className="flex min-w-0 items-center gap-2">
                <EntryIcon entry={entry} />
                <span className="truncate text-light-text dark:text-dark-text">{entry.name}</span>
              </span>
            </span>
          )
        }

        if (column.key === 'size') {
          return (
            <span key={column.key} className={definition.className}>
              <span className="font-mono text-uxs text-light-text-soft dark:text-dark-text-soft">
                <SizeValue entry={entry} />
              </span>
            </span>
          )
        }

        if (column.key === 'items') {
          return (
            <span key={column.key} className={definition.className}>
              <span className="font-mono text-uxs text-light-text-muted dark:text-dark-text-muted">
                {entry.isDir ? (entry.itemCount ?? '—') : '—'}
              </span>
            </span>
          )
        }

        if (column.key === 'type') {
          return (
            <span key={column.key} className={definition.className}>
              <span className="truncate text-usm text-light-text-muted dark:text-dark-text-muted">
                {entry.typeLabel}
              </span>
            </span>
          )
        }

        const value = column.key === 'created' ? formatDate(entry.createdAt) : formatDate(entry.modifiedAt)
        return (
          <span key={column.key} className={definition.className}>
            <span className="font-mono text-uxs text-light-text-muted dark:text-dark-text-muted">{value}</span>
          </span>
        )
      })}
    </button>
  )
}

function formatDate(value: string | null) {
  if (!value) {
    return '—'
  }

  return value.slice(0, 10)
}
