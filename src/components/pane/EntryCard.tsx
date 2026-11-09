import { memo, useMemo } from 'react'
import { getUnixTime, parseISO } from 'date-fns'
import type { ReactNode, RefObject } from 'react'
import { entryInteractionProps } from './entry-interaction'
import { EntryIcon } from '@/components/icons/EntryIcon'
import { useRenameInputFocus, type FileRowActions } from './FileRow'
import { SizeValue } from './SizeValue'
import { InlineRenameInput } from './InlineRenameInput'
import { cardHighlightClass, cutFadeClass, itemCountLabel } from './entry-classes'
import type { DirectoryEntry } from '@/lib/types/ipc'
import { thumbnailFingerprintKey, useThumbnailStore } from '@/stores/thumbnail-store'
import { FOCUS_RING_INSET } from '@/lib/focus-ring'
import { TEXT_SECONDARY } from '@/lib/class-names'
import type { InlineRenameDraft } from '@/stores/inline-rename-store'

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
  rename?: InlineRenameDraft
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
    ...entryInteractionProps(entry, actions),
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
  rename: InlineRenameDraft
  actions: FileRowActions
}

function CardRenameCaption({ inputRef, entry, rename, actions }: CardRenameCaptionProps) {
  return (
    <div className="min-w-0">
      <InlineRenameInput
        inputRef={inputRef}
        entryName={entry.name}
        value={rename.value}
        busy={rename.busy}
        actions={actions}
        className="w-full"
      />
      <CardRenameError message={rename.error} />
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
  rename,
  isDropTarget = false,
  draggable = false,
  thumbnail,
}: EntryCardProps) {
  const renameInputRef = useRenameInputFocus(rename !== undefined, entry.isDir)

  const stateClassName = cardHighlightClass({ isActivePane, isSelected, isFocused, isDropTarget })
  const contentClassName = cutFadeClass(isCut)
  const minimumCellClassName = minimumCellClassNames[mode]
  const sharedProps = cardInteractionProps(entry, actions, {
    draggable,
    rowIndex,
    columnIndex,
    isSelected,
  })
  const caption = rename ? (
    <CardRenameCaption inputRef={renameInputRef} entry={entry} rename={rename} actions={actions} />
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
