import { useRef, useState } from 'react'
import type { KeyboardEvent, PointerEvent as ReactPointerEvent } from 'react'
import { FOCUS_RING } from '@/lib/focus-ring'

type ResizeHandleProps = {
  /**
   * `vertical` (default) is a column divider dragged left/right; `horizontal` is
   * a row divider dragged up/down.
   */
  orientation?: 'vertical' | 'horizontal'
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
  const [dragging, setDragging] = useState(false)
  const pointerId = useRef<number | null>(null)
  const startPos = useRef(0)
  const horizontal = orientation === 'horizontal'

  function onPointerDown(event: ReactPointerEvent<HTMLDivElement>) {
    if (event.button !== 0) {
      return
    }
    event.preventDefault()
    pointerId.current = event.pointerId
    startPos.current = horizontal ? event.clientY : event.clientX
    event.currentTarget.setPointerCapture(event.pointerId)
    onDragStart()
    setDragging(true)
  }

  function onPointerMove(event: ReactPointerEvent<HTMLDivElement>) {
    if (pointerId.current === null) {
      return
    }
    onResize((horizontal ? event.clientY : event.clientX) - startPos.current)
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
    onCommit()
  }

  function onKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    const [back, forward] = horizontal ? ['ArrowUp', 'ArrowDown'] : ['ArrowLeft', 'ArrowRight']
    if (event.key === back) {
      event.preventDefault()
      onStep(-1)
      onCommit()
    } else if (event.key === forward) {
      event.preventDefault()
      onStep(1)
      onCommit()
    }
  }

  const restLine = showRestLine ? 'bg-light-divider dark:bg-dark-divider' : 'bg-transparent'

  return (
    <div
      role="separator"
      aria-orientation={orientation}
      aria-label={ariaLabel}
      aria-valuenow={Math.round(value)}
      aria-valuemin={Math.round(min)}
      aria-valuemax={Math.round(max)}
      tabIndex={0}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={endDrag}
      onPointerCancel={endDrag}
      onKeyDown={onKeyDown}
      // Zero layout width keeps the fixed pane geometry pixel-exact; the line and
      // grab zone are painted as absolute overlays around it.
      className={`group relative z-10 shrink-0 touch-none select-none ${
        horizontal ? 'h-0 w-full cursor-row-resize' : 'h-full w-0 cursor-col-resize'
      } ${FOCUS_RING}`}
    >
      <span
        className={`pointer-events-none absolute ${
          horizontal ? 'inset-x-0 top-0 h-px' : 'inset-y-0 left-0 w-px'
        } group-hover:bg-light-accent dark:group-hover:bg-dark-accent ${
          dragging ? 'bg-light-accent dark:bg-dark-accent' : restLine
        }`}
      />
      {/* Invisible grab zone that overhangs the line on both sides. */}
      <span
        className={`absolute ${horizontal ? 'inset-x-0 -top-1 -bottom-1' : 'inset-y-0 -left-1 -right-1'}`}
      />
    </div>
  )
}
