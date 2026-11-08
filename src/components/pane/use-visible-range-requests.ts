import { useCallback, useEffect, useRef } from 'react'
import { isTrashPath } from '@/lib/trash'
import type { DirectoryEntry } from '@/lib/types/ipc'
import { useNativeMenuWarmStore } from '@/stores/native-menu-warm-store'
import { usePanesStore } from '@/stores/panes-store'
import type { PaneId } from '@/types/pane'

const VISIBLE_RANGE_REQUEST_DEBOUNCE_MS = 100

type VisibleRangeOptions = {
  paneId: PaneId
  path: string
  entries: DirectoryEntry[]
}

export function useVisibleRangeRequests({ paneId, path, entries }: VisibleRangeOptions) {
  const requestVisibleItemCounts = usePanesStore((state) => state.requestVisibleItemCounts)
  const requestVisibleIcons = usePanesStore((state) => state.requestVisibleIcons)
  const warmVisibleNativeMenus = useNativeMenuWarmStore((state) => state.warmVisibleNativeMenus)
  const requestTimerRef = useRef<number | undefined>(undefined)

  useEffect(() => () => window.clearTimeout(requestTimerRef.current), [])

  return useCallback(
    (start: number, end: number, viewportCount: number) => {
      window.clearTimeout(requestTimerRef.current)
      requestTimerRef.current = window.setTimeout(() => {
        if (!isTrashPath(path)) void requestVisibleItemCounts(paneId, start, end, viewportCount)
        const visiblePaths = entries.slice(start, end + 1).map((entry) => entry.path)
        void requestVisibleIcons(paneId, visiblePaths)
        void warmVisibleNativeMenus(paneId, visiblePaths)
      }, VISIBLE_RANGE_REQUEST_DEBOUNCE_MS)
    },
    [entries, path, paneId, requestVisibleIcons, requestVisibleItemCounts, warmVisibleNativeMenus],
  )
}
