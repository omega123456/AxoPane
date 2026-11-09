import { CopyIcon, CornerUpRightIcon } from 'lucide-react'
import { useDragCursorStore } from '@/stores/drag-cursor-store'
import { useLayoutStore } from '@/stores/layout-store'
import { percentToFraction } from '@/lib/numbers'
import { TEXT_SECONDARY } from '@/lib/class-names'

export function DragCursorBadge() {
  const cursor = useDragCursorStore((state) => state.cursor)
  const zoom = useLayoutStore((state) => percentToFraction(Number(state.zoom)))

  if (!cursor) {
    return null
  }

  const copying = cursor.kind === 'copy'
  const Icon = copying ? CopyIcon : CornerUpRightIcon

  return (
    <div
      className="pointer-events-none fixed left-0 top-0 z-50 translate-x-4 translate-y-5 will-change-transform"
      style={{ transform: `translate3d(${cursor.x / zoom}px, ${cursor.y / zoom}px, 0)` }}
      aria-hidden="true"
    >
      <span className="flex items-center gap-1 rounded-control border border-light-flyout-stroke bg-light-card px-1.5 py-0.5 text-caption font-medium text-light-text dark:border-dark-flyout-stroke dark:bg-dark-card dark:text-dark-text">
        <Icon className={`h-3 w-3 shrink-0 ${TEXT_SECONDARY}`} />
        {copying ? 'Copy' : 'Move'}
      </span>
    </div>
  )
}
