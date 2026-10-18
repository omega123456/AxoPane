import { memo, useEffect, useMemo, useRef } from 'react'
import { getUnixTime, parseISO } from 'date-fns'
import type { DragEvent, KeyboardEvent, MouseEvent } from 'react'
import { EntryIcon } from '@/components/icons/EntryIcon'
import { focusRenameInput, type FileRowActions } from './FileRow'
import { SizeValue } from './SizeValue'
import type { DirectoryEntry } from '@/lib/types/ipc'
import { thumbnailFingerprintKey, useThumbnailStore } from '@/stores/thumbnail-store'
import { FOCUS_RING, FOCUS_RING_INSET } from '@/lib/focus-ring'

export type EntryCardThumbnail = {
  state: 'ready' | 'loading' | 'unavailable' | 'failed'
  quality?: 'low' | 'high' | null
  dataUrl?: string | null
}

function ThumbnailPreview({
  entry,
  override,
}: {
  entry: DirectoryEntry
  override?: EntryCardThumbnail
}) {
  const fingerprint = useMemo(() => {
    if (entry.isDir || entry.sizeBytes === null || entry.modifiedAt === null) return null
    const modifiedUnixSeconds = getUnixTime(parseISO(entry.modifiedAt))
    return Number.isFinite(modifiedUnixSeconds)
      ? thumbnailFingerprintKey({
          path: entry.path,
          modifiedUnixSeconds,
          sizeBytes: entry.sizeBytes,
        })
      : null
  }, [entry.isDir, entry.modifiedAt, entry.path, entry.sizeBytes])
  const record = useThumbnailStore((state) => (fingerprint ? state.cache[fingerprint] : undefined))
  const thumbnail =
    override ??
    record ??
    ({ state: fingerprint ? 'loading' : 'unavailable', quality: null } as const)
  const previewDataUrl = thumbnail.state === 'ready' ? (thumbnail.dataUrl ?? undefined) : undefined

  return (
    <div className="relative flex h-thumbnail-preview w-full shrink-0 items-center justify-center overflow-hidden rounded-control bg-light-recessed dark:bg-dark-recessed">
      {previewDataUrl ? (
        <img
          role="img"
          src={previewDataUrl}
          alt=""
          decoding="async"
          className="size-full object-contain"
        />
      ) : (
        <EntryIcon entry={entry} className="size-10 shrink-0" />
      )}
      {thumbnail.state === 'loading' ? (
        <span
          aria-label="Thumbnail loading"
          className="absolute inset-0 bg-light-skeleton dark:bg-dark-skeleton"
        />
      ) : null}
    </div>
  )
}

type EntryCardProps = {
  entry: DirectoryEntry
  mode: 'icons' | 'thumbnails'
  isActivePane: boolean
  isFocused: boolean
  isSelected: boolean
  actions: FileRowActions
  rowIndex: number
  columnIndex: number
  isCut?: boolean
  isRenaming?: boolean
  renameValue?: string
  renameBusy?: boolean
  renameError?: string | null
  isDropTarget?: boolean
  draggable?: boolean
  thumbnail?: EntryCardThumbnail
}

function EntryCardImpl({
  entry,
  mode,
  isActivePane,
  isFocused,
  isSelected,
  actions,
  rowIndex,
  columnIndex,
  isCut = false,
  isRenaming = false,
  renameValue = '',
  renameBusy = false,
  renameError = null,
  isDropTarget = false,
  draggable = false,
  thumbnail,
}: EntryCardProps) {
  const renameInputRef = useRef<HTMLInputElement>(null)
  useEffect(() => {
    if (isRenaming) {
      focusRenameInput(renameInputRef.current, entry.isDir)
    }
  }, [entry.isDir, isRenaming])

  const selectedClassName = isActivePane
    ? 'border-light-accent bg-light-tint dark:border-dark-accent dark:bg-dark-tint'
    : 'border-transparent bg-light-neutral-selection dark:bg-dark-neutral-selection'
  const stateClassName = `rounded-overlay border ${
    isSelected || isDropTarget
      ? selectedClassName
      : 'border-transparent hover:bg-light-hover dark:hover:bg-dark-hover'
  } ${
    // A selected card's accent border already marks it; the cursor ring would double it.
    isFocused && isActivePane && !isSelected
      ? 'ring-1 ring-inset ring-light-accent dark:ring-dark-accent'
      : ''
  }`
  // Cut items fade their contents, not the selection fill behind them.
  const contentClassName = isCut ? 'opacity-50' : ''
  const meta = entry.isDir ? (
    entry.itemCount === null ? (
      '—'
    ) : (
      `${entry.itemCount} ${entry.itemCount === 1 ? 'item' : 'items'}`
    )
  ) : (
    <SizeValue entry={entry} />
  )
  const labels = (
    <>
      <span className="truncate text-row text-light-text dark:text-dark-text">{entry.name}</span>
      <span className="-mt-1 truncate text-caption text-light-text-secondary dark:text-dark-text-secondary">
        {meta}
      </span>
    </>
  )
  const minimumCellClassName = mode === 'icons' ? 'min-w-icon-cell' : 'min-w-thumbnail-cell'
  const sharedProps = {
    'data-entry-id': entry.id,
    draggable,
    onDragStart: (event: DragEvent<HTMLDivElement>) => actions.onDragStart(entry.id, event),
    onDragEnd: actions.onDragEnd,
    onDragEnter: (event: DragEvent<HTMLDivElement>) => actions.onDragEnter(entry.id, event),
    onDragOver: (event: DragEvent<HTMLDivElement>) => actions.onDragOver(entry.id, event),
    onDragLeave: () => actions.onDragLeave(entry.id),
    onDrop: (event: DragEvent<HTMLDivElement>) => actions.onDrop(entry.id, event),
    onMouseDown: actions.onPointerDown,
    onDoubleClick: (event: MouseEvent<HTMLDivElement>) =>
      actions.onActivate(entry.id, event.detail, event.timeStamp),
    onClick: (event: MouseEvent<HTMLDivElement>) => actions.onClick(entry.id, event),
    onContextMenu: (event: MouseEvent<HTMLDivElement>) => actions.onContextMenu(entry.id, event),
    onAuxClick: (event: MouseEvent<HTMLDivElement>) => {
      if (event.button === 1 && entry.isDir && !entry.trashId) {
        event.preventDefault()
        actions.onMiddleClick(entry.id)
      }
    },
    role: 'gridcell' as const,
    'aria-rowindex': rowIndex,
    'aria-colindex': columnIndex,
    'aria-selected': isSelected,
    'aria-label': entry.name,
    title: entry.name,
  }

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

  const renameControl = isRenaming ? (
    <div className="min-w-0">
      <input
        ref={renameInputRef}
        autoComplete="off"
        aria-label={`Rename ${entry.name}`}
        value={renameValue}
        disabled={renameBusy}
        onMouseDown={(event) => event.stopPropagation()}
        onBlur={actions.onRenameBlur}
        onChange={(event) => actions.onRenameChange(event.target.value)}
        onKeyDown={onRenameKeyDown}
        className={`w-full min-w-0 select-text rounded-control border border-light-accent dark:border-dark-accent bg-light-base px-2 py-1 font-mono text-caption text-light-text outline-none dark:bg-dark-base dark:text-dark-text ${FOCUS_RING}`}
      />
      {renameError ? (
        <span className="block truncate pt-1 text-caption text-light-warning dark:text-dark-warning">
          {renameError}
        </span>
      ) : null}
    </div>
  ) : null

  if (mode === 'icons') {
    return (
      <div
        {...sharedProps}
        className={`flex h-icon-tile ${minimumCellClassName} cursor-pointer items-center gap-2 p-1.5 ${FOCUS_RING_INSET} ${stateClassName}`}
      >
        <EntryIcon entry={entry} className={`size-entry-icon shrink-0 ${contentClassName}`} />
        <div className={`flex min-w-0 flex-1 flex-col gap-1.5 ${contentClassName}`}>
          {renameControl ?? labels}
        </div>
      </div>
    )
  }

  return (
    <div
      {...sharedProps}
      className={`flex h-thumbnail-card ${minimumCellClassName} cursor-pointer flex-col gap-1.5 p-1.5 ${FOCUS_RING_INSET} ${stateClassName}`}
    >
      <div className={`flex min-h-0 flex-col gap-1.5 ${contentClassName}`}>
        <ThumbnailPreview entry={entry} override={thumbnail} />
        {renameControl ?? labels}
      </div>
    </div>
  )
}

export const EntryCard = memo(EntryCardImpl)
