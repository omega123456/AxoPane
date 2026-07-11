import { useEffect, useLayoutEffect, useReducer, useState } from 'react'
import {
  Virtualizer,
  elementScroll,
  observeElementOffset,
  observeElementRect,
} from '@tanstack/react-virtual'

type ElementVirtualizerOptions = {
  count: number
  estimateSize: (index: number) => number
  getScrollElement: () => HTMLDivElement | null
  overscan?: number
}

export function useElementVirtualizer(options: ElementVirtualizerOptions) {
  const rerender = useReducer((value) => value + 1, 0)[1]
  const [instance] = useState(
    () =>
      new Virtualizer<HTMLDivElement, HTMLDivElement>({
        count: options.count,
        estimateSize: options.estimateSize,
        getScrollElement: options.getScrollElement,
        observeElementRect,
        observeElementOffset,
        scrollToFn: elementScroll,
        initialRect: {
          width: 0,
          height: 480,
        },
        overscan: options.overscan ?? 10,
        onChange: () => {
          rerender()
        },
      }),
  )

  instance.setOptions({
    count: options.count,
    estimateSize: options.estimateSize,
    getScrollElement: options.getScrollElement,
    observeElementRect,
    observeElementOffset,
    scrollToFn: elementScroll,
    initialRect: {
      width: 0,
      height: 480,
    },
    overscan: options.overscan ?? 10,
    onChange: () => {
      rerender()
    },
  })

  useEffect(() => instance._didMount(), [instance])
  useLayoutEffect(() => instance._willUpdate())

  // Deliberately no `instance.measure()` call here: `measure()` clears the
  // virtualizer's entire cached-size table (`itemSizeCache`), forcing every
  // already-measured row to be re-measured from scratch. `setOptions` above
  // already applies a changed `count` (e.g. a newly loaded sparse range page
  // bumping total rows) and is enough for `getVirtualItems`/`getTotalSize` to
  // reflect it on next read — for the fixed-height row estimators this app
  // uses (`estimateSize: () => rowHeightPx` / `treeRowHeight`), there is
  // nothing stale to correct. A full remeasure on every row-count change was
  // wasted work that scaled with total rendered rows on every page load
  // (Functional Requirement 11 / Phase 4 acceptance: "fixed-height row-count
  // changes must not trigger full measurement resets").

  return instance
}
