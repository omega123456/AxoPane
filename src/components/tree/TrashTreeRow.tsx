import { memo, type MouseEvent } from 'react'
import { Trash2Icon } from '@/components/icons'
import { detectPlatformOs } from '@/lib/keymap'

type TrashTreeRowProps = {
  isCurrent: boolean
  onNavigate: () => void
  onOpenTab: () => void
  onContextMenu: (event: MouseEvent) => void
}

function TrashTreeRowImpl({ isCurrent, onNavigate, onOpenTab, onContextMenu }: TrashTreeRowProps) {
  const trashLabel = detectPlatformOs() === 'windows' ? 'Recycle Bin' : 'Trash'

  return (
    <div
      role="treeitem"
      data-tree-row="trash"
      onContextMenu={onContextMenu}
      className={`flex h-tree-row items-center gap-1 rounded-tab pr-2 text-row hover:bg-light-hover dark:hover:bg-dark-hover ${
        isCurrent
          ? 'bg-accent-blue-soft text-accent-blue-light dark:text-accent-blue'
          : 'text-light-text-soft dark:text-dark-text-soft'
      }`}
      style={{ paddingLeft: '8px' }}
    >
      <span className="inline-flex h-6 w-6 flex-none items-center justify-center" aria-hidden="true" />
      <button
        type="button"
        onClick={onNavigate}
        onMouseDown={(event) => {
          if (event.button === 1) {
            event.preventDefault()
          }
        }}
        onAuxClick={(event) => {
          if (event.button === 1) {
            event.preventDefault()
            onOpenTab()
          }
        }}
        className="flex min-w-0 flex-1 cursor-pointer items-center gap-2 rounded-tab py-1 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border"
      >
        <Trash2Icon className="size-4 flex-none text-light-text-muted dark:text-dark-text-muted" />
        <span className="truncate">{trashLabel}</span>
      </button>
    </div>
  )
}

export const TrashTreeRow = memo(TrashTreeRowImpl)
