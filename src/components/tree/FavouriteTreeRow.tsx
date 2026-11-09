import { memo, useRef, useState, type DragEvent, type MouseEvent } from 'react'
import { EntryIcon } from '@/components/icons/EntryIcon'
import { useDragStore } from '@/stores/drag-store'
import { FOCUS_RING_INSET } from '@/lib/focus-ring'
import { activeFillClass, dropTargetRing, HOVER_ON_POINTER, PRIMARY_TEXT } from '@/lib/class-names'
import { SelectionPill } from '@/components/controls/SelectionPill'
import { canDropOnFavourite, dropOntoFavourite } from './favourite-drop'
import { middleClickOpenProps } from './middle-click-open'
import { ENTER_KEY, ESCAPE_KEY } from '@/lib/key-table'

export function favouriteLabel(path: string) {
  const trimmed = path.replace(/[\\/]+$/, '')
  return trimmed.split(/[\\/]/).filter(Boolean).at(-1) ?? (trimmed || path)
}

export type FavouriteRowActions = {
  onNavigate: (path: string) => void
  onOpenTab: (path: string) => void
  onContextMenu: (path: string, event: MouseEvent) => void
  onRename: (path: string, name: string) => void
  onRenameCancel: () => void
}

type Props = {
  path: string
  name?: string
  index: number
  isCurrent: boolean
  isRenaming: boolean
  actions: FavouriteRowActions
}

type FavouriteViewProps = {
  path: string
  label: string
  actions: FavouriteRowActions
}

function FavouriteRenameInput({ path, label, actions }: FavouriteViewProps) {
  const renameCancelled = useRef(false)
  return (
    <span className="flex h-full min-w-0 flex-1 items-center gap-2">
      <EntryIcon entry={{ name: favouriteLabel(path), isDir: true }} />
      <input
        autoFocus
        autoComplete="off"
        aria-label={`Rename favourite ${label}`}
        defaultValue={label}
        onFocus={(event) => {
          renameCancelled.current = false
          event.currentTarget.select()
        }}
        onBlur={(event) => {
          if (!renameCancelled.current) actions.onRename(path, event.currentTarget.value)
        }}
        onKeyDown={(event) => {
          event.stopPropagation()
          if (event.key === ENTER_KEY) {
            event.preventDefault()
            event.currentTarget.blur()
            return
          }
          if (event.key === ESCAPE_KEY) {
            event.preventDefault()
            renameCancelled.current = true
            actions.onRenameCancel()
          }
        }}
        className="min-w-0 flex-1 select-text rounded-control border border-light-accent bg-light-base px-1 text-row text-light-text outline-none dark:border-dark-accent dark:bg-dark-base dark:text-dark-text"
      />
    </span>
  )
}

function FavouriteLink({ path, name, label, actions }: FavouriteViewProps & { name?: string }) {
  return (
    <button
      type="button"
      title={name ? path : undefined}
      onClick={() => actions.onNavigate(path)}
      {...middleClickOpenProps(() => actions.onOpenTab(path))}
      className={`flex h-full min-w-0 flex-1 cursor-pointer items-center gap-2 rounded-control text-left ${FOCUS_RING_INSET}`}
    >
      <EntryIcon entry={{ name: favouriteLabel(path), isDir: true }} />
      <span className="truncate">{label}</span>
    </button>
  )
}

function FavouriteTreeRowImpl({ path, name, index, isCurrent, isRenaming, actions }: Props) {
  const [isDropTarget, setIsDropTarget] = useState(false)
  const label = name ?? favouriteLabel(path)

  function handleDragOver(event: DragEvent<HTMLDivElement>) {
    if (!canDropOnFavourite(useDragStore.getState().drag)) return
    event.preventDefault()
    event.dataTransfer.dropEffect = 'move'
    setIsDropTarget(true)
  }

  async function handleDrop(event: DragEvent<HTMLDivElement>) {
    event.preventDefault()
    const drag = useDragStore.getState().drag
    useDragStore.getState().end()
    setIsDropTarget(false)
    await dropOntoFavourite(drag, { path, index })
  }

  return (
    <div
      data-favourite-row={path}
      draggable={!isRenaming}
      onDragStart={(event) => {
        useDragStore.getState().begin({ kind: 'favourite', path })
        event.dataTransfer.effectAllowed = 'move'
        event.dataTransfer.setData('text/plain', path)
      }}
      onDragEnd={() => useDragStore.getState().end()}
      onDragOver={handleDragOver}
      onDragLeave={() => setIsDropTarget(false)}
      onDrop={(event) => void handleDrop(event)}
      onContextMenu={(event) => actions.onContextMenu(path, event)}
      className={`relative flex h-tree-row items-center gap-2 rounded-control pl-2 pr-2 text-row ${PRIMARY_TEXT} ${HOVER_ON_POINTER} ${activeFillClass(isCurrent)} ${dropTargetRing(isDropTarget)}`}
    >
      {isCurrent ? <SelectionPill /> : null}
      <span className="w-3 flex-none" aria-hidden="true" />
      {isRenaming ? (
        <FavouriteRenameInput path={path} label={label} actions={actions} />
      ) : (
        <FavouriteLink path={path} name={name} label={label} actions={actions} />
      )}
    </div>
  )
}

export const FavouriteTreeRow = memo(FavouriteTreeRowImpl)
