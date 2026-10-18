import { memo, type MouseEvent } from 'react'
import { Trash2Icon } from '@/components/icons'
import { detectPlatformOs } from '@/lib/keymap'
import { SelectionPill } from '@/components/controls/SelectionPill'
import { FOCUS_RING_INSET } from '@/lib/focus-ring'

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
      className={`relative flex h-tree-row items-center gap-2 rounded-control pl-2 pr-2 text-row text-light-text hover:bg-light-hover dark:text-dark-text dark:hover:bg-dark-hover ${
        isCurrent ? 'bg-light-hover dark:bg-dark-hover' : ''
      }`}
    >
      {isCurrent ? <SelectionPill /> : null}
      <span className="w-3 flex-none" aria-hidden="true" />
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
        className={`flex h-full min-w-0 flex-1 cursor-pointer items-center gap-2 rounded-control ${FOCUS_RING_INSET}`}
      >
        <Trash2Icon className="size-4 flex-none text-light-text-secondary dark:text-dark-text-secondary" />
        <span className="truncate">{trashLabel}</span>
      </button>
    </div>
  )
}

export const TrashTreeRow = memo(TrashTreeRowImpl)
