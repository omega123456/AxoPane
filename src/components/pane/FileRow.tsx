import type { ColumnKey, DirectoryEntry } from '@/lib/types/ipc'
import { columnCellClassName } from '@/lib/columns'
import { dateToneClassName, formatEntryDate, type FormatDateOptions } from '@/lib/date-format'
import { AlertTriangleIcon } from '@/components/icons'
import { EntryIcon } from '@/components/icons/EntryIcon'
import { useDateDisplay } from '@/lib/use-date-display'
import { useLayoutStore } from '@/stores/layout-store'
import { columnFlexStyle } from './HeaderRow'
import { SizeValue } from './SizeValue'
import { InlineRenameInput } from './InlineRenameInput'
import { cutFadeClass, rowDropTargetClass, rowHighlightClass } from './entry-classes'

import type { DragEvent, MouseEvent, ReactNode, RefObject } from 'react'
import { memo, useEffect, useMemo, useRef } from 'react'
import { entryInteractionProps } from './entry-interaction'
import { TEXT_SECONDARY } from '@/lib/class-names'
import type { InlineRenameDraft } from '@/stores/inline-rename-store'
import { EMPTY_VALUE } from '@/lib/format'

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
  onActivate: (entryId: string, clickCount: number, eventTimeStamp: number) => void
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

export function focusRenameInput(input: HTMLInputElement | null, isDirectory: boolean) {
  if (!input) return

  input.focus()
  const extensionIndex = isDirectory ? -1 : input.value.lastIndexOf('.')
  const lastCharacterIndex = input.value.length - 1
  const hasInnerExtension = extensionIndex > 0 && extensionIndex < lastCharacterIndex
  input.setSelectionRange(0, hasInnerExtension ? extensionIndex : input.value.length)
}

export function useRenameInputFocus(isRenaming: boolean, isDirectory: boolean) {
  const renameInputRef = useRef<HTMLInputElement>(null)
  useEffect(() => {
    if (!isRenaming) {
      return
    }

    focusRenameInput(renameInputRef.current, isDirectory)
  }, [isDirectory, isRenaming])
  return renameInputRef
}

export function useVisibleColumns() {
  const columns = useLayoutStore((state) => state.columns)
  return useMemo(() => columns.filter((column) => column.visible), [columns])
}

type DateDisplay = FormatDateOptions

type RenameCellState = InlineRenameDraft & {
  inputRef: RefObject<HTMLInputElement | null>
  actions: FileRowActions
}

const NO_RENAME_DRAFT: InlineRenameDraft = { value: '', busy: false, error: null }

type CellContext = {
  entry: DirectoryEntry
  dateDisplay: DateDisplay
  rename: RenameCellState
}

type CellRenderer = (context: CellContext) => ReactNode

function DateCellText({ value, dateDisplay }: { value: string | null; dateDisplay: DateDisplay }) {
  const formatted = formatEntryDate(value, dateDisplay)
  return (
    <span className={`text-caption ${dateToneClassName[formatted.tone]}`}>{formatted.text}</span>
  )
}

function ItemCountCellText({ entry }: { entry: DirectoryEntry }) {
  return (
    <span className={`font-mono text-caption ${TEXT_SECONDARY}`}>
      {entry.isDir ? (entry.itemCount ?? EMPTY_VALUE) : EMPTY_VALUE}
    </span>
  )
}

function SizeCellText({ entry }: { entry: DirectoryEntry }) {
  return (
    <span className={`font-mono text-caption ${TEXT_SECONDARY}`}>
      <SizeValue entry={entry} />
    </span>
  )
}

function RenameStatusText({ rename }: { rename: RenameCellState }) {
  if (rename.error) {
    return (
      <span className="flex items-center gap-1 text-caption text-light-warning dark:text-dark-warning">
        <AlertTriangleIcon className="h-3.5 w-3.5 shrink-0" />
        <span className="truncate">{rename.error}</span>
      </span>
    )
  }
  return (
    <span className={`truncate text-caption ${TEXT_SECONDARY}`}>
      {rename.busy ? 'Renaming…' : 'Rename'}
    </span>
  )
}

const sharedCells = {
  size: ({ entry }: CellContext) => <SizeCellText entry={entry} />,
  items: ({ entry }: CellContext) => <ItemCountCellText entry={entry} />,
  modified: ({ entry, dateDisplay }: CellContext) => (
    <DateCellText value={entry.modifiedAt} dateDisplay={dateDisplay} />
  ),
  created: ({ entry, dateDisplay }: CellContext) => (
    <DateCellText value={entry.createdAt} dateDisplay={dateDisplay} />
  ),
} satisfies Partial<Record<ColumnKey, CellRenderer>>

const browseCells: Record<ColumnKey, CellRenderer> = {
  ...sharedCells,
  name: ({ entry }) => (
    <span className="flex min-w-0 items-center gap-2">
      <EntryIcon entry={entry} />
      <span
        title={entry.originalPath ? `Original location: ${entry.originalPath}` : undefined}
        className="truncate text-light-text dark:text-dark-text"
      >
        {entry.name}
      </span>
    </span>
  ),
  type: ({ entry }) => (
    <span className={`truncate text-caption ${TEXT_SECONDARY}`}>{entry.typeLabel}</span>
  ),
}

const renameCells: Record<ColumnKey, CellRenderer> = {
  ...sharedCells,
  name: ({ entry, rename }) => (
    <span className="flex min-w-0 items-center gap-2">
      <EntryIcon entry={entry} />
      <InlineRenameInput
        inputRef={rename.inputRef}
        entryName={entry.name}
        value={rename.value}
        busy={rename.busy}
        actions={rename.actions}
        className="flex-1"
      />
    </span>
  ),
  type: ({ rename }) => <RenameStatusText rename={rename} />,
}

type FileRowProps = {
  entry: DirectoryEntry
  isActivePane: boolean
  isFocused: boolean
  isSelected: boolean
  actions: FileRowActions
  isCut?: boolean
  rename?: InlineRenameDraft
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
  rename,
  isDropTarget = false,
  draggable = false,
}: FileRowProps) {
  const columnWidths = useLayoutStore((state) => state.columnWidths)
  const visibleColumns = useVisibleColumns()
  const dateDisplay = useDateDisplay()
  const isRenaming = rename !== undefined
  const renameInputRef = useRenameInputFocus(isRenaming, entry.isDir)
  const renameCell = { inputRef: renameInputRef, ...(rename ?? NO_RENAME_DRAFT), actions }
  const cellRenderers = isRenaming ? renameCells : browseCells
  const rowClassName = `group flex h-row w-full items-center rounded-control px-2 text-row text-left focus-visible:outline-none ${rowHighlightClass({ isActivePane, isSelected, isFocused })}`
  const cells = visibleColumns.map((column) => (
    <span
      key={column.key}
      style={columnFlexStyle(column.key, columnWidths)}
      className={`${columnCellClassName(column.key)} ${cutFadeClass(isCut)}`}
    >
      {cellRenderers[column.key]({ entry, dateDisplay, rename: renameCell })}
    </span>
  ))

  if (isRenaming) {
    return (
      <div role="row" data-entry-id={entry.id} className={rowClassName}>
        {cells}
      </div>
    )
  }

  return (
    <div
      role="row"
      data-entry-id={entry.id}
      draggable={draggable}
      {...entryInteractionProps(entry, actions)}
      className={`${rowClassName} cursor-pointer select-none ${rowDropTargetClass(isDropTarget)}`}
    >
      {cells}
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
