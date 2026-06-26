import { useRef, useState } from 'react'
import type { KeyboardEvent, PointerEvent as ReactPointerEvent } from 'react'

type ResizeHandleProps = {
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
   * Reports how far (CSS px) the pointer has moved since the drag began. Drags
   * are relative to the grab point, so the divider never jumps to the cursor.
   */
  onResize: (deltaX: number) => void
  /** Nudges the divider by `delta` notches via the keyboard (±1 per arrow). */
  onStep: (delta: number) => void
  /** Persist the final size once the drag (or keyboard nudge) settles. */
  onCommit: () => void
}

/**
 * A vertical, draggable divider that resizes its neighbouring panes. It occupies
 * a single pixel of layout (so it never disturbs the fixed design geometry) but
 * exposes a wider invisible hit area for grabbing. Pointer capture keeps the drag
 * alive when the cursor outruns the thin line; arrow keys give the same control.
 */
export function ResizeHandle({
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
  const startX = useRef(0)

  function onPointerDown(event: ReactPointerEvent<HTMLDivElement>) {
    if (event.button !== 0) {
      return
    }
    event.preventDefault()
    pointerId.current = event.pointerId
    startX.current = event.clientX
    event.currentTarget.setPointerCapture(event.pointerId)
    onDragStart()
    setDragging(true)
  }

  function onPointerMove(event: ReactPointerEvent<HTMLDivElement>) {
    if (pointerId.current === null) {
      return
    }
    onResize(event.clientX - startX.current)
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
    if (event.key === 'ArrowLeft') {
      event.preventDefault()
      onStep(-1)
      onCommit()
    } else if (event.key === 'ArrowRight') {
      event.preventDefault()
      onStep(1)
      onCommit()
    }
  }

  const restLine = showRestLine ? 'bg-light-border dark:bg-dark-border' : 'bg-transparent'

  return (
    <div
      role="separator"
      aria-orientation="vertical"
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
      className="group relative z-10 h-full w-0 shrink-0 cursor-col-resize touch-none select-none focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border"
    >
      <span
        className={`pointer-events-none absolute inset-y-0 left-0 w-px group-hover:bg-accent-blue-border ${
          dragging ? 'bg-accent-blue-border' : restLine
        }`}
      />
      {/* Invisible grab zone that overhangs the line on both sides. */}
      <span className="absolute inset-y-0 -left-1 -right-1" />
    </div>
  )
}
