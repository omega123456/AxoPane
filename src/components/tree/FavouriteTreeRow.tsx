import { memo, useState, type DragEvent, type MouseEvent } from 'react'
import { EntryIcon } from '@/components/icons/EntryIcon'
import { pathsMatch } from '@/lib/path-compare'
import { useConfigStore } from '@/stores/config-store'
import { useDragStore } from '@/stores/drag-store'

export function favouriteLabel(path: string) {
  const trimmed = path.replace(/[\\/]+$/, '')
  return trimmed.split(/[\\/]/).filter(Boolean).at(-1) ?? (trimmed || path)
}

export type FavouriteRowActions = {
  onNavigate: (path: string) => void
  onOpenTab: (path: string) => void
  onContextMenu: (path: string, event: MouseEvent) => void
}

type Props = {
  path: string
  index: number
  isCurrent: boolean
  actions: FavouriteRowActions
}

function FavouriteTreeRowImpl({ path, index, isCurrent, actions }: Props) {
  const [isDropTarget, setIsDropTarget] = useState(false)

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
      draggable
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
      className={`flex h-tree-row items-center gap-2 rounded-tab px-3 text-row hover:bg-light-hover dark:hover:bg-dark-hover ${
        isCurrent
          ? 'bg-accent-blue-soft text-accent-blue-light dark:text-accent-blue'
          : 'text-light-text-soft dark:text-dark-text-soft'
      } ${isDropTarget ? 'ring-2 ring-inset ring-accent-blue-border' : ''}`}
    >
      <button
        type="button"
        onClick={() => actions.onNavigate(path)}
        onMouseDown={(event) => event.button === 1 && event.preventDefault()}
        onAuxClick={(event) => {
          if (event.button === 1) {
            event.preventDefault()
            actions.onOpenTab(path)
          }
        }}
        className="flex min-w-0 flex-1 cursor-pointer items-center gap-2 rounded-tab py-1 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border"
      >
        <EntryIcon entry={{ name: favouriteLabel(path), isDir: true }} />
        <span className="truncate">{favouriteLabel(path)}</span>
      </button>
    </div>
  )
}

export const FavouriteTreeRow = memo(FavouriteTreeRowImpl)
