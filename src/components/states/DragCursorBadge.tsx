import { CopyIcon, CornerUpRightIcon } from 'lucide-react'
import { useDragCursorStore } from '@/stores/drag-cursor-store'
import { useLayoutStore } from '@/stores/layout-store'

/**
 * Copy-vs-move feedback that follows the pointer during an OS-owned drag.
 *
 * The OS cursor badge cannot carry it: the drag session advertises Copy for its
 * whole lifetime (deliberately, so an external app never moves a file out of the
 * pane), and wry forces `NSDragOperation::Copy` when the page accepts nothing.
 * So the badge always reads "copy" no matter which modifier is held. This draws
 * the real outcome next to the cursor instead.
 */
export function DragCursorBadge() {
  const cursor = useDragCursorStore((state) => state.cursor)
  const zoom = useLayoutStore((state) => Number(state.zoom) / 100)

  if (!cursor) {
    return null
  }

  const copying = cursor.kind === 'copy'
  const Icon = copying ? CopyIcon : CornerUpRightIcon

  return (
    <div
      // Offset down-right so the badge clears the OS drag image, which is drawn
      // at the pointer itself. Positioning is per-frame data, hence inline.
      className="pointer-events-none fixed left-0 top-0 z-50 translate-x-4 translate-y-5 will-change-transform"
      // The cursor is in viewport pixels, but this element sits inside the root's
      // CSS `zoom`, which scales every length in the subtree — including a fixed
      // element's offsets. Dividing by the factor cancels that, so the badge
      // lands on the pointer at any zoom level.
      style={{ transform: `translate3d(${cursor.x / zoom}px, ${cursor.y / zoom}px, 0)` }}
      aria-hidden="true"
    >
      <span className="flex items-center gap-1 rounded-md border border-light-border-strong bg-light-surface px-1.5 py-0.5 text-uxs font-medium text-light-text dark:border-dark-border-strong dark:bg-dark-surface dark:text-dark-text">
        <Icon className="h-3 w-3 shrink-0 text-light-text-muted dark:text-dark-text-muted" />
        {copying ? 'Copy' : 'Move'}
      </span>
    </div>
  )
}
