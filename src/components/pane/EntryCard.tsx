import { memo, useMemo } from 'react'
import { getUnixTime, parseISO } from 'date-fns'
import type { DragEvent, MouseEvent, ReactNode, RefObject } from 'react'
import { isMiddleClickOpen } from '@/lib/mouse-buttons'
import { EntryIcon } from '@/components/icons/EntryIcon'
import { useRenameInputFocus, type FileRowActions } from './FileRow'
import { SizeValue } from './SizeValue'
import { InlineRenameInput } from './InlineRenameInput'
import { cardHighlightClass, cutFadeClass, itemCountLabel } from './entry-classes'
import type { DirectoryEntry } from '@/lib/types/ipc'
import { thumbnailFingerprintKey, useThumbnailStore } from '@/stores/thumbnail-store'
import { FOCUS_RING_INSET } from '@/lib/focus-ring'
import { TEXT_SECONDARY } from '@/lib/class-names'

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
    if (entry.isDir) return null
    if (entry.sizeBytes === null) return null
    if (entry.modifiedAt === null) return null
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

const minimumCellClassNames: Record<EntryCardProps['mode'], string> = {
  icons: 'min-w-icon-cell',
  thumbnails: 'min-w-thumbnail-cell',
}

function CardLabels({ entry }: { entry: DirectoryEntry }) {
  return (
    <>
      <span className="truncate text-row text-light-text dark:text-dark-text">{entry.name}</span>
      <span className={`-mt-1 truncate text-caption ${TEXT_SECONDARY}`}>
        {entry.isDir ? itemCountLabel(entry.itemCount) : <SizeValue entry={entry} />}
      </span>
    </>
  )
}

function CardRenameError({ message }: { message: string | null }) {
  if (!message) {
    return null
  }
  return (
    <span className="block truncate pt-1 text-caption text-light-warning dark:text-dark-warning">
      {message}
    </span>
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

type CardGridPosition = {
  draggable: boolean
  rowIndex: number
  columnIndex: number
  isSelected: boolean
}

function cardInteractionProps(
  entry: DirectoryEntry,
  actions: FileRowActions,
  position: CardGridPosition,
) {
  return {
    'data-entry-id': entry.id,
    draggable: position.draggable,
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
      if (isMiddleClickOpen(event, entry)) {
        event.preventDefault()
        actions.onMiddleClick(entry.id)
      }
    },
    role: 'gridcell' as const,
    'aria-rowindex': position.rowIndex,
    'aria-colindex': position.columnIndex,
    'aria-selected': position.isSelected,
    'aria-label': entry.name,
    title: entry.name,
  }
}

type CardRenameCaptionProps = {
  inputRef: RefObject<HTMLInputElement | null>
  entry: DirectoryEntry
  value: string
  busy: boolean
  error: string | null
  actions: FileRowActions
}

function CardRenameCaption({
  inputRef,
  entry,
  value,
  busy,
  error,
  actions,
}: CardRenameCaptionProps) {
  return (
    <div className="min-w-0">
      <InlineRenameInput
        inputRef={inputRef}
        entryName={entry.name}
        value={value}
        busy={busy}
        actions={actions}
        className="w-full"
      />
      <CardRenameError message={error} />
    </div>
  )
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
  const renameInputRef = useRenameInputFocus(isRenaming, entry.isDir)

  const stateClassName = cardHighlightClass({ isActivePane, isSelected, isFocused, isDropTarget })
  const contentClassName = cutFadeClass(isCut)
  const minimumCellClassName = minimumCellClassNames[mode]
  const sharedProps = cardInteractionProps(entry, actions, {
    draggable,
    rowIndex,
    columnIndex,
    isSelected,
  })
  const caption = isRenaming ? (
    <CardRenameCaption
      inputRef={renameInputRef}
      entry={entry}
      value={renameValue}
      busy={renameBusy}
      error={renameError}
      actions={actions}
    />
  ) : (
    <CardLabels entry={entry} />
  )

  return (
    <CardLayout
      mode={mode}
      interactionProps={sharedProps}
      className={`${minimumCellClassName} cursor-pointer p-1.5 ${FOCUS_RING_INSET} ${stateClassName}`}
      contentClassName={contentClassName}
      entry={entry}
      thumbnail={thumbnail}
      caption={caption}
    />
  )
}

type CardLayoutProps = {
  mode: EntryCardProps['mode']
  interactionProps: ReturnType<typeof cardInteractionProps>
  className: string
  contentClassName: string
  entry: DirectoryEntry
  thumbnail?: EntryCardThumbnail
  caption: ReactNode
}

function CardLayout({
  mode,
  interactionProps,
  className,
  contentClassName,
  entry,
  thumbnail,
  caption,
}: CardLayoutProps) {
  if (mode === 'icons') {
    return (
      <div {...interactionProps} className={`flex h-icon-tile items-center gap-2 ${className}`}>
        <EntryIcon entry={entry} className={`size-entry-icon shrink-0 ${contentClassName}`} />
        <div className={`flex min-w-0 flex-1 flex-col gap-1.5 ${contentClassName}`}>{caption}</div>
      </div>
    )
  }

  return (
    <div {...interactionProps} className={`flex h-thumbnail-card flex-col gap-1.5 ${className}`}>
      <div className={`flex min-h-0 flex-col gap-1.5 ${contentClassName}`}>
        <ThumbnailPreview entry={entry} override={thumbnail} />
        {caption}
      </div>
    </div>
  )
}

export const EntryCard = memo(EntryCardImpl)
