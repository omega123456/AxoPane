import { memo, useEffect, useMemo, useRef } from 'react'
import { getUnixTime, parseISO } from 'date-fns'
import type { DragEvent, KeyboardEvent, MouseEvent } from 'react'
import { EntryIcon } from '@/components/icons/EntryIcon'
import type { FileRowActions } from './FileRow'
import type { DirectoryEntry } from '@/lib/types/ipc'
import { thumbnailFingerprintKey, useThumbnailStore } from '@/stores/thumbnail-store'

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
    <div className="relative flex size-thumbnail-preview shrink-0 items-center justify-center self-center overflow-hidden rounded-tab bg-light-panel dark:bg-dark-panel">
      {previewDataUrl ? (
        <img
          role="img"
          src={previewDataUrl}
          alt=""
          decoding="async"
          className="size-full object-contain"
        />
      ) : (
        <EntryIcon entry={entry} className="size-8 shrink-0" />
      )}
      {thumbnail.state === 'loading' ? (
        <span
          aria-label="Thumbnail loading"
          className="absolute inset-0 animate-pulse bg-light-skeleton dark:bg-dark-skeleton"
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
      renameInputRef.current?.focus()
      renameInputRef.current?.select()
    }
  }, [isRenaming])

  const stateClassName = `${isSelected ? 'bg-accent-blue-soft' : 'bg-light-surface dark:bg-dark-surface'} ${isFocused && isActivePane ? 'ring-2 ring-inset ring-accent-blue-border' : ''} ${isDropTarget ? 'ring-2 ring-inset ring-accent-blue-border bg-accent-blue-soft' : ''} ${isCut ? 'opacity-50' : ''}`
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
        aria-label={`Rename ${entry.name}`}
        value={renameValue}
        disabled={renameBusy}
        onMouseDown={(event) => event.stopPropagation()}
        onBlur={actions.onRenameBlur}
        onChange={(event) => actions.onRenameChange(event.target.value)}
        onKeyDown={onRenameKeyDown}
        className="w-full min-w-0 select-text rounded-tab border border-accent-blue-border bg-light-window px-2 py-1 font-mono text-uxs text-light-text outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:bg-dark-window dark:text-dark-text"
      />
      {renameError ? (
        <span className="block truncate pt-1 text-uxs text-accent-amber">{renameError}</span>
      ) : null}
    </div>
  ) : null

  if (mode === 'icons') {
    return (
      <div
        {...sharedProps}
        className={`flex h-icon-tile ${minimumCellClassName} cursor-pointer items-center gap-gap rounded-tab px-2 text-row hover:bg-light-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent-blue-border dark:hover:bg-dark-hover ${stateClassName}`}
      >
        <EntryIcon entry={entry} className="size-entry-icon shrink-0" />
        {renameControl ?? (
          <span className="min-w-0 truncate text-light-text dark:text-dark-text">{entry.name}</span>
        )}
      </div>
    )
  }

  return (
    <div
      {...sharedProps}
      className={`flex h-thumbnail-card ${minimumCellClassName} cursor-pointer flex-col rounded-tab p-2 hover:bg-light-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent-blue-border dark:hover:bg-dark-hover ${stateClassName}`}
    >
      <ThumbnailPreview entry={entry} override={thumbnail} />
      <div className="min-h-0 w-full flex-1 pt-2 text-center text-row text-light-text dark:text-dark-text">
        {renameControl ?? <span className="line-clamp-2 break-words">{entry.name}</span>}
      </div>
    </div>
  )
}

export const EntryCard = memo(EntryCardImpl)
