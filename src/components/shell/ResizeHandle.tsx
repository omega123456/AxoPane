import { useRef, useState } from 'react'
import type { PointerEvent as ReactPointerEvent } from 'react'
import { FOCUS_RING } from '@/lib/focus-ring'
import { handleKeyByTable } from '@/lib/key-table'
import { PRIMARY_MOUSE_BUTTON } from '@/lib/mouse-buttons'

type ResizeOrientation = 'vertical' | 'horizontal'

type OrientationClasses = {
  handle: string
  line: string
  grabZone: string
}

const ORIENTATION_CLASSES: Record<ResizeOrientation, OrientationClasses> = {
  horizontal: {
    handle: 'h-0 w-full cursor-row-resize',
    line: 'inset-x-0 top-0 h-px',
    grabZone: 'inset-x-0 -top-1 -bottom-1',
  },
  vertical: {
    handle: 'h-full w-0 cursor-col-resize',
    line: 'inset-y-0 left-0 w-px',
    grabZone: 'inset-y-0 -left-1 -right-1',
  },
}

function dividerLineClass(isDragging: boolean, showRestLine: boolean) {
  if (isDragging) {
    return 'bg-light-accent dark:bg-dark-accent'
  }
  return showRestLine ? 'bg-light-divider dark:bg-dark-divider' : 'bg-transparent'
}

type ResizeHandleProps = {
  /**
   * `vertical` (default) is a column divider dragged left/right; `horizontal` is
   * a row divider dragged up/down.
   */
  orientation?: ResizeOrientation
  /** Accessible label describing what the divider resizes. */
  ariaLabel: string
  /** Current value, exposed via `aria-valuenow` for assistive tech. */
  value: number
  min: number
  max: number
  /**
   * Whether to paint the 1px divider line at rest. The folder-tree handle sits
   * on top of the tree's own `border-r`, so it stays transparent until hovered;
   * the inter-pane handle is the only separator there, so it shows the line.
   */
  showRestLine?: boolean
  /** Snapshot the size to anchor the drag against (called on pointer down). */
  onDragStart: () => void
  /**
   * Reports how far (CSS px) the pointer has moved along the drag axis since the drag began. Drags
   * are relative to the grab point, so the divider never jumps to the cursor.
   */
  onResize: (deltaX: number) => void
  /** Nudges the divider by `delta` notches via the keyboard (±1 per arrow; left/up is -1). */
  onStep: (delta: number) => void
  /** Persist the final size once the drag (or keyboard nudge) settles. */
  onCommit: () => void
}

type DragCallbacks = Pick<ResizeHandleProps, 'onDragStart' | 'onResize' | 'onCommit'>

function usePointerDrag(horizontal: boolean, callbacks: DragCallbacks) {
  const [dragging, setDragging] = useState(false)
  const pointerId = useRef<number | null>(null)
  const startPos = useRef(0)
  const axisPosition = (event: ReactPointerEvent<HTMLDivElement>) =>
    horizontal ? event.clientY : event.clientX

  function onPointerDown(event: ReactPointerEvent<HTMLDivElement>) {
    if (event.button !== PRIMARY_MOUSE_BUTTON) {
      return
    }
    event.preventDefault()
    pointerId.current = event.pointerId
    startPos.current = axisPosition(event)
    event.currentTarget.setPointerCapture(event.pointerId)
    callbacks.onDragStart()
    setDragging(true)
  }

  function onPointerMove(event: ReactPointerEvent<HTMLDivElement>) {
    if (pointerId.current === null) {
      return
    }
    callbacks.onResize(axisPosition(event) - startPos.current)
  }

  function endDrag(event: ReactPointerEvent<HTMLDivElement>) {
    if (pointerId.current === null) {
      return
    }
    // The capture was taken on pointerdown for this exact pointer, so releasing
    // it here is always valid.
    event.currentTarget.releasePointerCapture(event.pointerId)
    pointerId.current = null
    setDragging(false)
    callbacks.onCommit()
  }

  return { dragging, onPointerDown, onPointerMove, endDrag }
}

function stepKeyTable(horizontal: boolean, onStep: (delta: number) => void, onCommit: () => void) {
  const [back, forward] = horizontal ? ['ArrowUp', 'ArrowDown'] : ['ArrowLeft', 'ArrowRight']
  const stepBy = (delta: number) => () => {
    onStep(delta)
    onCommit()
  }
  return { [back]: stepBy(-1), [forward]: stepBy(1) }
}

/**
 * A draggable divider that resizes its neighbouring panes. It occupies
 * a single pixel of layout (so it never disturbs the fixed design geometry) but
 * exposes a wider invisible hit area for grabbing. Pointer capture keeps the drag
 * alive when the cursor outruns the thin line; arrow keys give the same control.
 */
export function ResizeHandle({
  orientation = 'vertical',
  ariaLabel,
  value,
  min,
  max,
  showRestLine = true,
  onDragStart,
  onResize,
  onStep,
  onCommit,
}: ResizeHandleProps) {
  const horizontal = orientation === 'horizontal'
  const drag = usePointerDrag(horizontal, { onDragStart, onResize, onCommit })
  const orientationClasses = ORIENTATION_CLASSES[orientation]

  return (
    <div
      role="separator"
      aria-orientation={orientation}
      aria-label={ariaLabel}
      aria-valuenow={Math.round(value)}
      aria-valuemin={Math.round(min)}
      aria-valuemax={Math.round(max)}
      tabIndex={0}
      onPointerDown={drag.onPointerDown}
      onPointerMove={drag.onPointerMove}
      onPointerUp={drag.endDrag}
      onPointerCancel={drag.endDrag}
      onKeyDown={(event) => handleKeyByTable(event, stepKeyTable(horizontal, onStep, onCommit))}
      // Zero layout width keeps the fixed pane geometry pixel-exact; the line and
      // grab zone are painted as absolute overlays around it.
      className={`group relative z-10 shrink-0 touch-none select-none ${orientationClasses.handle} ${FOCUS_RING}`}
    >
      <span
        className={`pointer-events-none absolute ${orientationClasses.line} group-hover:bg-light-accent dark:group-hover:bg-dark-accent ${dividerLineClass(drag.dragging, showRestLine)}`}
      />
      {/* Invisible grab zone that overhangs the line on both sides. */}
      <span className={`absolute ${orientationClasses.grabZone}`} />
    </div>
  )
}
