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
  orientation?: ResizeOrientation
  ariaLabel: string
  value: number
  min: number
  max: number
  showRestLine?: boolean
  onDragStart: () => void
  onResize: (deltaX: number) => void
  onStep: (delta: number) => void
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
      className={`group relative z-10 shrink-0 touch-none select-none ${orientationClasses.handle} ${FOCUS_RING}`}
    >
      <span
        className={`pointer-events-none absolute ${orientationClasses.line} group-hover:bg-light-accent dark:group-hover:bg-dark-accent ${dividerLineClass(drag.dragging, showRestLine)}`}
      />

      <span className={`absolute ${orientationClasses.grabZone}`} />
    </div>
  )
}
