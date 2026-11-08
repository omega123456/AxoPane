import { useEffect, useRef, useState, type MouseEvent, type RefObject } from 'react'
import { documentZoom } from '@/components/menus/menu-geometry'
import type { PaneId } from '@/types/pane'
import type { MarqueeRect } from './MarqueeSelection'
import {
  marqueeRectBetween,
  marqueeSelection,
  type ContentPoint,
  type MarqueeDrag,
} from './marquee-selection'
import { PRIMARY_MOUSE_BUTTON } from '@/lib/mouse-buttons'
import { hasPrimaryModifier } from '@/lib/key-table'

const ROW_SELECTOR = '[role="row"], [role="gridcell"]'

type MarqueeSelectionOptions = {
  paneId: PaneId
  scrollContainerRef: RefObject<HTMLDivElement | null>
  selectedIds: string[]
  entryIdsInRect: (rect: MarqueeRect) => string[]
  onPointerDown: () => void
  setSelection: (
    paneId: PaneId,
    selectedIds: string[],
    anchorId: string | null,
    focusedId: string | null,
  ) => void
  setFocusedEntry: (paneId: PaneId, entryId: string | null) => void
}

function startsOnRow(event: MouseEvent<HTMLDivElement>) {
  return event.target instanceof Element && event.target.closest(ROW_SELECTOR) !== null
}

function contentPointIn(
  container: HTMLDivElement | null,
  clientX: number,
  clientY: number,
): ContentPoint | null {
  if (!container) return null
  const bounds = container.getBoundingClientRect()
  const zoom = documentZoom()
  return {
    x: (clientX - bounds.left) / zoom + container.scrollLeft,
    y: (clientY - bounds.top) / zoom + container.scrollTop,
  }
}

export function useMarqueeSelection(options: MarqueeSelectionOptions) {
  const { paneId, scrollContainerRef, setSelection, setFocusedEntry } = options
  const [marqueeRect, setMarqueeRect] = useState<MarqueeRect | null>(null)
  const detachListenersRef = useRef<(() => void) | null>(null)

  useEffect(() => () => detachListenersRef.current?.(), [])

  function applySelection(drag: MarqueeDrag, rect: MarqueeRect) {
    const result = marqueeSelection(drag, options.entryIdsInRect(rect))
    setSelection(paneId, result.selectedIds, result.anchorId, result.focusedId)
    if (result.focusedId) {
      setFocusedEntry(paneId, result.focusedId)
    }
  }

  function trackDrag(drag: MarqueeDrag) {
    function onMouseMove(moveEvent: globalThis.MouseEvent) {
      const point = contentPointIn(scrollContainerRef.current, moveEvent.clientX, moveEvent.clientY)
      if (!point) {
        return
      }
      const rect = marqueeRectBetween(drag.start, point)
      setMarqueeRect(rect)
      if (rect) {
        applySelection(drag, rect)
      }
    }

    function detach() {
      document.removeEventListener('mousemove', onMouseMove)
      document.removeEventListener('mouseup', onMouseUp)
    }

    function onMouseUp() {
      setMarqueeRect(null)
      detach()
      detachListenersRef.current = null
    }

    document.addEventListener('mousemove', onMouseMove)
    document.addEventListener('mouseup', onMouseUp)
    detachListenersRef.current = detach
  }

  function handleContainerMouseDown(event: MouseEvent<HTMLDivElement>) {
    options.onPointerDown()
    if (event.button !== PRIMARY_MOUSE_BUTTON) {
      return
    }
    if (startsOnRow(event)) {
      return
    }

    const start = contentPointIn(scrollContainerRef.current, event.clientX, event.clientY)
    if (!start) {
      return
    }

    const additive = hasPrimaryModifier(event)
    if (!additive) {
      setSelection(paneId, [], null, null)
    }
    trackDrag({ start, additive, baseSelectedIds: options.selectedIds })
  }

  return { marqueeRect, handleContainerMouseDown }
}
