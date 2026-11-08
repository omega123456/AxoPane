import { useMemo, useRef } from 'react'
import type { CSSProperties, PointerEvent as ReactPointerEvent } from 'react'
import { columnDefinitions } from '@/lib/columns'
import { persistAppConfig } from '@/lib/app-config'
import type { PaneState } from '@/types/pane'
import { ChevronDownIcon } from '@/components/icons'
import { isTrashPath } from '@/lib/trash'
import { defaultColumnWidths, useLayoutStore } from '@/stores/layout-store'
import { usePanesStore } from '@/stores/panes-store'
import type { ColumnConfig, ColumnKey, SortDirection } from '@/lib/types/ipc'
import { handleKeyByTable, type KeyHandlerTable } from '@/lib/key-table'
import { FOCUS_RING } from '@/lib/focus-ring'
import { TEXT_SECONDARY } from '@/lib/class-names'

const COLUMN_RESIZE_KEYBOARD_STEP_PX = 8

const paneColumnGapPx = 0
/** 3px list inset + 8px row padding on each side (the header uses 11px directly). */
const paneRowHorizontalPaddingPx = 22

const ariaSortByDirection: Record<SortDirection, 'ascending' | 'descending'> = {
  asc: 'ascending',
  desc: 'descending',
}

function SortChevron({ active, direction }: { active: boolean; direction: SortDirection }) {
  if (!active) {
    return null
  }
  return (
    <ChevronDownIcon className={`h-3 w-3 shrink-0 ${direction === 'asc' ? 'rotate-180' : ''}`} />
  )
}

type HeaderRowProps = {
  pane: PaneState
}

type ColumnResizeHandlers = {
  onPointerDown: (event: ReactPointerEvent<HTMLDivElement>, key: ColumnKey) => void
  onPointerMove: (event: ReactPointerEvent<HTMLDivElement>) => void
  onPointerEnd: (event: ReactPointerEvent<HTMLDivElement>) => void
  keyTable: (key: ColumnKey) => KeyHandlerTable
}

function useColumnResize(): ColumnResizeHandlers {
  const columnWidths = useLayoutStore((state) => state.columnWidths)
  const setColumnWidth = useLayoutStore((state) => state.setColumnWidth)
  const dragRef = useRef<{ key: ColumnKey; startX: number; startWidth: number } | null>(null)
  const widthOf = (key: ColumnKey) => columnWidths[key] ?? defaultColumnWidths[key]

  function nudgeColumnWidth(key: ColumnKey, deltaPx: number) {
    setColumnWidth(key, widthOf(key) + deltaPx)
    void persistAppConfig()
  }

  return {
    keyTable: (key) => ({
      ArrowLeft: () => nudgeColumnWidth(key, -COLUMN_RESIZE_KEYBOARD_STEP_PX),
      ArrowRight: () => nudgeColumnWidth(key, COLUMN_RESIZE_KEYBOARD_STEP_PX),
    }),
    onPointerDown: (event, key) => {
      event.preventDefault()
      event.stopPropagation()
      dragRef.current = { key, startX: event.clientX, startWidth: widthOf(key) }
      event.currentTarget.setPointerCapture(event.pointerId)
    },
    onPointerMove: (event) => {
      const drag = dragRef.current
      if (!drag) {
        return
      }
      setColumnWidth(drag.key, drag.startWidth + event.clientX - drag.startX)
    },
    onPointerEnd: (event) => {
      if (!dragRef.current) {
        return
      }
      event.currentTarget.releasePointerCapture(event.pointerId)
      dragRef.current = null
      void persistAppConfig()
    },
  }
}

export function HeaderRow({ pane }: HeaderRowProps) {
  const configuredColumns = useLayoutStore((state) => state.columns)
  const columnWidths = useLayoutStore((state) => state.columnWidths)
  const resize = useColumnResize()
  const columns = useMemo(
    () => configuredColumns.filter((column) => column.visible),
    [configuredColumns],
  )
  const contentWidth = paneContentWidth(columns, columnWidths)
  const isTrash = isTrashPath(pane.path)
  const itemsSortPending = pane.sortKey === 'items' && pane.itemsSortStatus === 'counting'

  return (
    <div
      style={{ minWidth: `${contentWidth}px` }}
      className={`group/header flex h-headrow w-full items-center border-b border-light-divider px-2.75 text-caption ${TEXT_SECONDARY} dark:border-dark-divider`}
    >
      {columns.map((column) => (
        <ColumnHeader
          key={column.key}
          pane={pane}
          columnKey={column.key}
          label={
            isTrash && column.key === 'modified' ? 'Deleted' : columnDefinitions[column.key].label
          }
          showItemsSortStatus={column.key === 'items' && itemsSortPending}
          columnWidths={columnWidths}
          resize={resize}
        />
      ))}
    </div>
  )
}

type ColumnHeaderProps = {
  pane: PaneState
  columnKey: ColumnKey
  label: string
  showItemsSortStatus: boolean
  columnWidths: Partial<Record<ColumnKey, number>>
  resize: ColumnResizeHandlers
}

function ColumnHeader({
  pane,
  columnKey,
  label,
  showItemsSortStatus,
  columnWidths,
  resize,
}: ColumnHeaderProps) {
  const setSort = usePanesStore((state) => state.setSort)
  const active = pane.sortKey === columnKey
  const definition = columnDefinitions[columnKey]
  const statusId = `${pane.id}-items-sort-status`
  return (
    <div
      style={columnFlexStyle(columnKey, columnWidths)}
      role="columnheader"
      aria-label={label}
      className={`${definition.className} ${columnKey === 'name' ? 'pl-6' : ''} group relative flex h-full items-center`}
      aria-sort={active ? ariaSortByDirection[pane.sortDirection] : 'none'}
    >
      <button
        type="button"
        onClick={() => void setSort(pane.id, columnKey)}
        aria-describedby={showItemsSortStatus ? statusId : undefined}
        className={`flex h-full w-full min-w-0 cursor-pointer items-center gap-2 rounded-control ${definition.align === 'right' ? 'justify-end' : ''} ${FOCUS_RING}`}
      >
        <span className="truncate">{label}</span>
        <SortChevron active={active} direction={pane.sortDirection} />
      </button>
      {showItemsSortStatus ? (
        <span id={statusId} className="sr-only">
          Counting items. Current row order will update when counting finishes.
        </span>
      ) : null}
      <div
        role="separator"
        aria-label={`Resize ${label} column`}
        aria-orientation="vertical"
        tabIndex={0}
        onPointerDown={(event) => resize.onPointerDown(event, columnKey)}
        onPointerMove={resize.onPointerMove}
        onPointerUp={resize.onPointerEnd}
        onPointerCancel={resize.onPointerEnd}
        onKeyDown={(event) => handleKeyByTable(event, resize.keyTable(columnKey))}
        className={`absolute -right-1 top-0 h-full w-2 cursor-col-resize touch-none ${FOCUS_RING}`}
      >
        {/* Column dividers appear while the header is hovered, as in Explorer. */}
        <span className="absolute inset-y-1 left-1 w-px bg-light-divider opacity-0 group-hover/header:opacity-100 dark:bg-dark-divider" />
      </div>
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

export function paneContentWidth(
  columns: ColumnConfig[],
  widths: Partial<Record<ColumnKey, number>>,
) {
  const columnWidth = columns.reduce(
    (total, column) => total + (widths[column.key] ?? defaultColumnWidths[column.key]),
    0,
  )
  const gapWidth = Math.max(0, columns.length - 1) * paneColumnGapPx
  return paneRowHorizontalPaddingPx + columnWidth + gapWidth
}
