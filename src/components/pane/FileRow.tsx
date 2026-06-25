import type { DirectoryEntry } from '@/lib/types/ipc'
import { columnDefinitions } from '@/lib/columns'
import { FileIcon, FolderIcon } from '@/components/icons'
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
}

import type { MouseEvent } from 'react'
import { useMemo } from 'react'

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
}: FileRowProps) {
  const columns = useLayoutStore((state) => state.columns)
  const visibleColumns = useMemo(() => columns.filter((column) => column.visible), [columns])

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
      className={`group flex h-row w-full items-center gap-3 border-b border-light-border px-3 text-row text-left focus-visible:outline-none dark:border-dark-border ${
        isSelected ? 'bg-accent-blue-soft' : 'bg-light-surface dark:bg-dark-surface'
      } ${isFocused && isActivePane ? 'ring-2 ring-inset ring-accent-blue-border' : ''} hover:bg-light-hover dark:hover:bg-dark-hover`}
    >
      {visibleColumns.map((column) => {
        const definition = columnDefinitions[column.key]

        if (column.key === 'name') {
          return (
            <span key={column.key} className={definition.className}>
              <span className="flex min-w-0 items-center gap-2">
                {entry.isDir ? (
                  <FolderIcon className="h-4 w-4 shrink-0 text-accent-blue-light dark:text-accent-blue" />
                ) : (
                  <FileIcon className="h-4 w-4 shrink-0 text-accent-blue-light dark:text-accent-blue" />
                )}
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
