import { useEffect, useLayoutEffect, useReducer, useState } from 'react'
import {
  Virtualizer,
  elementScroll,
  observeElementOffset,
  observeElementRect,
} from '@tanstack/react-virtual'

const INITIAL_VIEWPORT_HEIGHT_PX = 480
const DEFAULT_OVERSCAN_ROWS = 10

type ElementVirtualizerOptions = {
  count: number
  estimateSize: (index: number) => number
  getScrollElement: () => HTMLDivElement | null
  overscan?: number
  padding?: number
  measurementKey?: string | number
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
          height: INITIAL_VIEWPORT_HEIGHT_PX,
        },
        overscan: options.overscan ?? DEFAULT_OVERSCAN_ROWS,
        paddingStart: options.padding ?? 0,
        paddingEnd: options.padding ?? 0,
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
      height: INITIAL_VIEWPORT_HEIGHT_PX,
    },
    overscan: options.overscan ?? DEFAULT_OVERSCAN_ROWS,
    paddingStart: options.padding ?? 0,
    paddingEnd: options.padding ?? 0,
    onChange: () => {
      rerender()
    },
  })

  useEffect(() => instance._didMount(), [instance])
  useLayoutEffect(() => instance._willUpdate())

  useLayoutEffect(() => {
    instance.measure()
  }, [instance, options.measurementKey])

  return instance
}
