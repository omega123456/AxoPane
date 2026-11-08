import {
  useCallback,
  useEffect,
  useImperativeHandle,
  useLayoutEffect,
  useMemo,
  useRef,
  type ForwardedRef,
  type RefObject,
} from 'react'
import { useElementVirtualizer } from '@/lib/use-element-virtualizer'
import { useLayoutStore } from '@/stores/layout-store'
import { BOTH_SIDES } from '@/lib/numbers'
import { documentZoom } from '@/components/menus/menu-geometry'
import { virtualListHeight, virtualRowsOrFallback } from '@/lib/virtual-rows'
import { paneContentWidth } from './HeaderRow'
import type { DetailsViewHandle } from './DetailsView'

/** Rows rendered before the virtualizer has measured its scroll element. */
const UNMEASURED_FALLBACK_ROWS = 30
const FALLBACK_ROW_KEY_PREFIX = 'row'
export const DETAILS_ROW_HEIGHT_PX = 28
/** Blank band above the first row and below the last, matching the 3px side inset. */
export const DETAILS_LIST_INSET_PX = 3
const rowHeightPx = DETAILS_ROW_HEIGHT_PX

const DETAILS_OVERSCAN_ROWS = 10

export function useDetailsRows(rowCount: number, parentRef: RefObject<HTMLDivElement | null>) {
  const rowVirtualizer = useElementVirtualizer({
    count: rowCount,
    getScrollElement: () => parentRef.current,
    estimateSize: () => rowHeightPx,
    overscan: DETAILS_OVERSCAN_ROWS,
    padding: DETAILS_LIST_INSET_PX,
  })
  const itemsToRender = virtualRowsOrFallback({
    measuredRows: rowVirtualizer.getVirtualItems(),
    rowCount,
    fallbackRowLimit: UNMEASURED_FALLBACK_ROWS,
    rowStart: (index) => DETAILS_LIST_INSET_PX + index * rowHeightPx,
    keyPrefix: FALLBACK_ROW_KEY_PREFIX,
  })
  const totalHeight = virtualListHeight({
    measuredHeight: rowVirtualizer.getTotalSize(),
    rowCount,
    estimated: rowCount * rowHeightPx + BOTH_SIDES * DETAILS_LIST_INSET_PX,
  })
  const scrollToIndex = useCallback(
    (index: number) => rowVirtualizer.scrollToIndex(index),
    [rowVirtualizer],
  )
  return { scrollToIndex, itemsToRender, totalHeight }
}

export function useVisibleContentWidth() {
  const configuredColumns = useLayoutStore((state) => state.columns)
  const columnWidths = useLayoutStore((state) => state.columnWidths)
  return useMemo(
    () =>
      paneContentWidth(
        configuredColumns.filter((column) => column.visible),
        columnWidths,
      ),
    [columnWidths, configuredColumns],
  )
}

export function useVisibleRangeReport(
  itemsToRender: Array<{ index: number }>,
  parentOffset: number,
  onVisibleRangeChange: (start: number, end: number, viewportCount: number) => void,
) {
  useEffect(() => {
    if (itemsToRender.length === 0) return
    const start = Math.max(0, itemsToRender[0].index - parentOffset)
    const end = Math.max(0, itemsToRender[itemsToRender.length - 1].index - parentOffset)
    onVisibleRangeChange(start, end, Math.max(1, end - start + 1))
  }, [itemsToRender, onVisibleRangeChange, parentOffset])
}

function nextHorizontalScrollLeft(horizontalScroll: HTMLDivElement, deltaX: number) {
  const maxScrollLeft = Math.max(0, horizontalScroll.scrollWidth - horizontalScroll.clientWidth)
  return Math.max(0, Math.min(maxScrollLeft, horizontalScroll.scrollLeft + deltaX))
}

export function useDetachedHorizontalScroll(
  usesDetachedMacScrollbars: boolean,
  totalHeight: number,
) {
  const headerScrollRef = useRef<HTMLDivElement | null>(null)
  const horizontalScrollRef = useRef<HTMLDivElement | null>(null)
  const contentLayerRef = useRef<HTMLDivElement | null>(null)
  const horizontalScrollLeftRef = useRef(0)

  const syncHorizontalScroll = useCallback((scrollLeft: number) => {
    horizontalScrollLeftRef.current = scrollLeft
    if (headerScrollRef.current) headerScrollRef.current.scrollLeft = scrollLeft
    if (contentLayerRef.current)
      contentLayerRef.current.style.transform =
        scrollLeft === 0 ? '' : `translateX(-${scrollLeft}px)`
  }, [])

  function handleBodyWheel(event: React.WheelEvent<HTMLDivElement>) {
    if (!usesDetachedMacScrollbars) return
    if (event.deltaX === 0) return

    const horizontalScroll = horizontalScrollRef.current
    if (!horizontalScroll) return

    const nextScrollLeft = nextHorizontalScrollLeft(horizontalScroll, event.deltaX)
    if (nextScrollLeft !== horizontalScroll.scrollLeft) {
      horizontalScroll.scrollLeft = nextScrollLeft
      syncHorizontalScroll(nextScrollLeft)
    }
  }

  function syncAttachedHeaderScroll(scrollLeft: number) {
    if (usesDetachedMacScrollbars) return
    if (!headerScrollRef.current) return
    headerScrollRef.current.scrollLeft = scrollLeft
  }

  useLayoutEffect(() => {
    if (!usesDetachedMacScrollbars) {
      horizontalScrollLeftRef.current = 0
      if (contentLayerRef.current) contentLayerRef.current.style.transform = ''
      return
    }
    syncHorizontalScroll(horizontalScrollRef.current?.scrollLeft ?? 0)
  }, [syncHorizontalScroll, totalHeight, usesDetachedMacScrollbars])

  return {
    headerScrollRef,
    horizontalScrollRef,
    contentLayerRef,
    horizontalScrollLeftRef,
    syncHorizontalScroll,
    syncAttachedHeaderScroll,
    handleBodyWheel,
  }
}

type DetailsHandleSources = {
  parentRef: RefObject<HTMLDivElement | null>
  horizontalScrollLeftRef: RefObject<number>
  scrollToIndex: (index: number) => void
  usesDetachedMacScrollbars: boolean
}

export function useDetailsHandle(
  ref: ForwardedRef<DetailsViewHandle>,
  sources: DetailsHandleSources,
) {
  const { parentRef, horizontalScrollLeftRef, scrollToIndex, usesDetachedMacScrollbars } = sources
  useImperativeHandle(
    ref,
    () => ({
      scrollToRow: (index) => scrollToIndex(index),
      getVisibleRowCount: () =>
        Math.max(1, Math.floor((parentRef.current?.clientHeight ?? 0) / rowHeightPx)),
      contentPoint: (clientX, clientY) => {
        const container = parentRef.current
        if (!container) return null
        const bounds = container.getBoundingClientRect()
        const zoom = documentZoom()
        return {
          x:
            (clientX - bounds.left) / zoom +
            (usesDetachedMacScrollbars ? horizontalScrollLeftRef.current : container.scrollLeft),
          y: (clientY - bounds.top) / zoom + container.scrollTop,
        }
      },
    }),
    [parentRef, horizontalScrollLeftRef, scrollToIndex, usesDetachedMacScrollbars],
  )
}
