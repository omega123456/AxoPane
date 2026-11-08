import { useMemo } from 'react'
import { ArrowUpIcon, FolderGlyph } from '@/components/icons'
import { columnCellClassName } from '@/lib/columns'
import { useLayoutStore } from '@/stores/layout-store'
import { columnFlexStyle } from './HeaderRow'
import { NO_CLASS, TEXT_SECONDARY } from '@/lib/class-names'
import type { ColumnConfig } from '@/lib/types/ipc'

type ParentRowProps = {
  isActivePane: boolean
  isFocused: boolean
  onPointerDown: () => void
  onActivate: (eventTimeStamp?: number) => void
  onFocus: () => void
}

function parentRowFocusRing(showsFocusRing: boolean) {
  return showsFocusRing ? 'ring-1 ring-inset ring-light-accent dark:ring-dark-accent' : NO_CLASS
}

function ParentRowCell({ columnKey }: { columnKey: ColumnConfig['key'] }) {
  if (columnKey === 'name') {
    return (
      <span className="flex min-w-0 items-center gap-2">
        <span className="relative flex h-4 w-4 shrink-0 items-center justify-center">
          <FolderGlyph className="h-4 w-4" />
          <ArrowUpIcon className="absolute top-1.5 h-2.5 w-2.5 text-light-text dark:text-dark-base" />
        </span>
        <span className={`truncate ${TEXT_SECONDARY}`}>..</span>
      </span>
    )
  }
  if (columnKey === 'type') {
    return <span className={`truncate text-caption ${TEXT_SECONDARY}`}>Parent folder</span>
  }
  return <span className={`font-mono text-caption ${TEXT_SECONDARY}`}>—</span>
}

/**
 * Synthetic ".." row rendered at the top of a pane's listing whenever the
 * current folder has a parent. Activating it (Enter / double-click) navigates
 * to the parent directory. Double-clicks are detected by pairing raw click
 * timestamps in FilePane rather than via dblclick, whose click counting is
 * unreliable across consecutive up-navigations. It is intentionally never
 * selectable for copy/move or folder-size operations — it carries no entry id.
 */
export function ParentRow({
  isActivePane,
  isFocused,
  onPointerDown,
  onActivate,
  onFocus,
}: ParentRowProps) {
  const columns = useLayoutStore((state) => state.columns)
  const columnWidths = useLayoutStore((state) => state.columnWidths)
  const visibleColumns = useMemo(() => columns.filter((column) => column.visible), [columns])
  const showsFocusRing = isFocused && isActivePane

  return (
    <button
      type="button"
      role="row"
      data-parent-row="true"
      aria-label="Go to parent folder"
      onMouseDown={(event) => {
        event.preventDefault()
        onPointerDown()
      }}
      onClick={(event) => {
        onFocus()
        // detail 0 = keyboard-synthesized click (Space on the button); only
        // real pointer clicks participate in double-click pairing.
        if (event.detail > 0) {
          onActivate(event.timeStamp)
        }
      }}
      onKeyDown={(event) => {
        if (event.key === 'Enter') {
          event.preventDefault()
          onActivate()
        }
      }}
      className={`group flex h-row w-full cursor-pointer items-center rounded-control px-2 text-row text-left focus-visible:outline-none ${parentRowFocusRing(showsFocusRing)} hover:bg-light-hover dark:hover:bg-dark-hover`}
    >
      {visibleColumns.map((column) => (
        <span
          key={column.key}
          style={columnFlexStyle(column.key, columnWidths)}
          className={columnCellClassName(column.key)}
        >
          <ParentRowCell columnKey={column.key} />
        </span>
      ))}
    </button>
  )
}
