import { memo, useRef, useState, type DragEvent, type MouseEvent } from 'react'
import { EntryIcon } from '@/components/icons/EntryIcon'
import { pathsMatch } from '@/lib/path-compare'
import { useConfigStore } from '@/stores/config-store'
import { useDragStore } from '@/stores/drag-store'
import { FOCUS_RING_INSET } from '@/lib/focus-ring'
import { SelectionPill } from '@/components/controls/SelectionPill'

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
  /** Custom display name, when the user renamed this favourite. */
  name?: string
  index: number
  isCurrent: boolean
  isRenaming: boolean
  actions: FavouriteRowActions
}

function FavouriteTreeRowImpl({ path, name, index, isCurrent, isRenaming, actions }: Props) {
  const [isDropTarget, setIsDropTarget] = useState(false)
  // Escape unmounts the input, which can still fire blur; that blur must not commit.
  const renameCancelled = useRef(false)
  const label = name ?? favouriteLabel(path)

  function canAcceptDrop() {
    const drag = useDragStore.getState().drag
    return drag?.kind === 'favourite' || drag?.items.some((item) => item.isDir) === true
  }

  function handleDragOver(event: DragEvent<HTMLDivElement>) {
    if (!canAcceptDrop()) return
    event.preventDefault()
    event.dataTransfer.dropEffect = 'move'
    setIsDropTarget(true)
  }

  async function handleDrop(event: DragEvent<HTMLDivElement>) {
    event.preventDefault()
    const drag = useDragStore.getState().drag
    useDragStore.getState().end()
    setIsDropTarget(false)
    if (drag?.kind === 'favourite') {
      if (!pathsMatch(drag.path, path))
        await useConfigStore.getState().reorderFavourite(drag.path, index)
      return
    }
    if (drag?.kind === 'file-transfer') {
      let insertAt = index
      for (const item of drag.items) {
        if (item.isDir) {
          await useConfigStore.getState().addFavourite(item.path, insertAt)
          insertAt += 1
        }
      }
    }
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
      className={`relative flex h-tree-row items-center gap-2 rounded-control pl-2 pr-2 text-row text-light-text hover:bg-light-hover dark:text-dark-text dark:hover:bg-dark-hover ${
        isCurrent ? 'bg-light-hover dark:bg-dark-hover' : ''
      } ${isDropTarget ? 'ring-2 ring-inset ring-light-accent dark:ring-dark-accent' : ''}`}
    >
      {isCurrent ? <SelectionPill /> : null}
      <span className="w-3 flex-none" aria-hidden="true" />
      {isRenaming ? (
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
              if (event.key === 'Enter') {
                event.preventDefault()
                event.currentTarget.blur()
              } else if (event.key === 'Escape') {
                event.preventDefault()
                renameCancelled.current = true
                actions.onRenameCancel()
              }
            }}
            className="min-w-0 flex-1 select-text rounded-control border border-light-accent bg-light-base px-1 text-row text-light-text outline-none dark:border-dark-accent dark:bg-dark-base dark:text-dark-text"
          />
        </span>
      ) : (
        <button
          type="button"
          title={name ? path : undefined}
          onClick={() => actions.onNavigate(path)}
          onMouseDown={(event) => event.button === 1 && event.preventDefault()}
          onAuxClick={(event) => {
            if (event.button === 1) {
              event.preventDefault()
              actions.onOpenTab(path)
            }
          }}
          className={`flex h-full min-w-0 flex-1 cursor-pointer items-center gap-2 rounded-control text-left ${FOCUS_RING_INSET}`}
        >
          <EntryIcon entry={{ name: favouriteLabel(path), isDir: true }} />
          <span className="truncate">{label}</span>
        </button>
      )}
    </div>
  )
}

export const FavouriteTreeRow = memo(FavouriteTreeRowImpl)
