import { useEffect, useRef } from 'react'
import { buildContextMenuContent } from '@/components/menus/menu-definitions'
import { Trash2Icon } from '@/components/icons'
import { detectPlatformOs } from '@/lib/keymap'
import { TRASH_PATH } from '@/lib/trash'
import { useContextMenuStore } from '@/stores/context-menu-store'
import { usePanesStore } from '@/stores/panes-store'

export function TrashTreeRow() {
  const trashLabel = detectPlatformOs() === 'windows' ? 'Recycle Bin' : 'Trash'
  const activePaneId = usePanesStore((state) => state.activePaneId)
  const activePath = usePanesStore((state) => state.panes[activePaneId].path)
  const navigatePane = usePanesStore((state) => state.navigatePane)
  const openTabFromPath = usePanesStore((state) => state.openTabFromPath)
  const openMenu = useContextMenuStore((state) => state.openMenu)
  const rowRef = useRef<HTMLDivElement>(null)

  const isCurrent = activePath === TRASH_PATH

  useEffect(() => {
    if (isCurrent) {
      rowRef.current?.scrollIntoView({ block: 'nearest' })
    }
  }, [isCurrent])

  return (
    <li>
      <div
        ref={rowRef}
        onContextMenu={(event) => {
          event.preventDefault()
          openMenu({
            paneId: activePaneId,
            title: trashLabel,
            chip: 'DIR',
            x: event.clientX,
            y: event.clientY,
            ...buildContextMenuContent(
              activePaneId,
              { kind: 'tree', path: TRASH_PATH },
              detectPlatformOs(),
            ),
          })
        }}
        className={`flex items-center gap-1 rounded-tab pr-2 text-row ${
          isCurrent
            ? 'bg-accent-blue-soft text-accent-blue-light dark:text-accent-blue'
            : 'text-light-text-soft dark:text-dark-text-soft'
        }`}
        style={{ paddingLeft: '8px' }}
      >
        <span className="inline-flex h-6 w-6 flex-none items-center justify-center" aria-hidden="true" />
        <button
          type="button"
          onClick={() => void navigatePane(activePaneId, TRASH_PATH)}
          onMouseDown={(event) => {
            if (event.button === 1) {
              event.preventDefault()
            }
          }}
          onAuxClick={(event) => {
            if (event.button === 1) {
              event.preventDefault()
              void openTabFromPath(activePaneId, TRASH_PATH)
            }
          }}
          className="flex min-w-0 flex-1 cursor-pointer items-center gap-2 rounded-tab py-1 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border"
        >
          <Trash2Icon className="size-4 flex-none text-light-text-muted dark:text-dark-text-muted" />
          <span className="truncate">{trashLabel}</span>
        </button>
      </div>
    </li>
  )
}
