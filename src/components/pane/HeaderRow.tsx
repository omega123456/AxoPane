import { useMemo, useRef } from 'react'
import type { CSSProperties, PointerEvent as ReactPointerEvent } from 'react'
import { columnDefinitions } from '@/lib/columns'
import { persistAppConfig } from '@/lib/app-config'
import type { PaneState } from '@/types/pane'
import { ChevronDownIcon } from '@/components/icons'
import { isTrashPath } from '@/lib/trash'
import { defaultColumnWidths, useLayoutStore } from '@/stores/layout-store'
import { usePanesStore } from '@/stores/panes-store'
import type { ColumnKey } from '@/lib/types/ipc'

type HeaderRowProps = {
  pane: PaneState
}

export function HeaderRow({ pane }: HeaderRowProps) {
  const setSort = usePanesStore((state) => state.setSort)
  const configuredColumns = useLayoutStore((state) => state.columns)
  const columnWidths = useLayoutStore((state) => state.columnWidths)
  const setColumnWidth = useLayoutStore((state) => state.setColumnWidth)
  const dragRef = useRef<{ key: ColumnKey; startX: number; startWidth: number } | null>(null)
  const columns = useMemo(
    () => configuredColumns.filter((column) => column.visible),
    [configuredColumns],
  )
  const isTrash = isTrashPath(pane.path)

  function startResize(event: ReactPointerEvent<HTMLDivElement>, key: ColumnKey) {
    event.preventDefault()
    event.stopPropagation()
    dragRef.current = {
      key,
      startX: event.clientX,
      startWidth: columnWidths[key] ?? defaultColumnWidths[key],
    }
    event.currentTarget.setPointerCapture(event.pointerId)
  }

  function resizeColumn(event: ReactPointerEvent<HTMLDivElement>) {
    const drag = dragRef.current
    if (!drag) {
      return
    }

    setColumnWidth(drag.key, drag.startWidth + event.clientX - drag.startX)
  }

  function endResize(event: ReactPointerEvent<HTMLDivElement>) {
    if (!dragRef.current) {
      return
    }

    event.currentTarget.releasePointerCapture(event.pointerId)
    dragRef.current = null
    void persistAppConfig()
  }

  return (
    <div className="flex h-headrow items-center gap-3 border-b border-light-border bg-light-header px-3 text-uxs uppercase tracking-wide text-light-text-muted dark:border-dark-border dark:bg-dark-header dark:text-dark-text-muted">
      {columns.map((column) => {
        const active = pane.sortKey === column.key
        const definition = columnDefinitions[column.key]
        const label = isTrash && column.key === 'modified' ? 'Deleted' : definition.label
        return (
          <div
            key={column.key}
            style={columnFlexStyle(column.key, columnWidths)}
            className={`${definition.className} group relative flex h-full items-center`}
          >
            <button
              type="button"
              onClick={() => void setSort(pane.id, column.key)}
              className="flex h-full w-full min-w-0 cursor-pointer items-center gap-1 rounded-tab text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border"
            >
              <span className="truncate">{label}</span>
              {active ? (
                <ChevronDownIcon
                  className={`ml-auto h-3.5 w-3.5 shrink-0 ${pane.sortDirection === 'asc' ? 'rotate-180' : ''} text-accent-blue-light dark:text-accent-blue`}
                />
              ) : null}
            </button>
            <div
              role="separator"
              aria-label={`Resize ${label} column`}
              aria-orientation="vertical"
              tabIndex={0}
              onPointerDown={(event) => startResize(event, column.key)}
              onPointerMove={resizeColumn}
              onPointerUp={endResize}
              onPointerCancel={endResize}
              onKeyDown={(event) => {
                if (event.key === 'ArrowLeft') {
                  event.preventDefault()
                  setColumnWidth(
                    column.key,
                    (columnWidths[column.key] ?? defaultColumnWidths[column.key]) - 8,
                  )
                  void persistAppConfig()
                } else if (event.key === 'ArrowRight') {
                  event.preventDefault()
                  setColumnWidth(
                    column.key,
                    (columnWidths[column.key] ?? defaultColumnWidths[column.key]) + 8,
                  )
                  void persistAppConfig()
                }
              }}
              className="absolute -right-1 top-0 h-full w-2 cursor-col-resize touch-none focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border"
            >
              <span className="absolute inset-y-1 left-1 w-px bg-light-border dark:bg-dark-border" />
            </div>
          </div>
        )
      })}
    </div>
  )
}

export function columnFlexStyle(
  key: ColumnKey,
  widths: Partial<Record<ColumnKey, number>>,
): CSSProperties {
  const width = widths[key] ?? defaultColumnWidths[key]
  return { flex: `0 0 ${width}px`, width: `${width}px` }
}
