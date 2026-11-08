import { useCallback, useLayoutEffect, useRef, type RefObject } from 'react'
import type { PaneId } from '@/types/pane'
import {
  armedScrollRestore,
  scrollRestoreAfterAttempt,
  type PendingScrollRestore,
} from './scroll-restore'

type ScrollRestoreOptions = {
  paneId: PaneId
  path: string
  savedScrollTop: number
  entryCount: number
  scrollContainerRef: RefObject<HTMLDivElement | null>
  setScrollPosition: (paneId: PaneId, path: string, scrollTop: number) => void
}

function applyRestoreTarget(scrollElement: HTMLElement, target: number) {
  if (scrollElement.scrollTop !== target) {
    scrollElement.scrollTop = target
  }
}

export function useScrollRestore({
  paneId,
  path,
  savedScrollTop,
  entryCount,
  scrollContainerRef,
  setScrollPosition,
}: ScrollRestoreOptions) {
  const scrollPathRef = useRef(path)
  const liveScrollTopRef = useRef(0)
  const pendingRestoreRef = useRef<PendingScrollRestore | null>(armedScrollRestore(savedScrollTop))

  useLayoutEffect(() => {
    const scrollElement = scrollContainerRef.current
    if (!scrollElement) {
      return
    }

    const previousPath = scrollPathRef.current
    if (previousPath !== path) {
      setScrollPosition(paneId, previousPath, liveScrollTopRef.current)
      scrollPathRef.current = path
      pendingRestoreRef.current = armedScrollRestore(savedScrollTop)
    }

    const pendingRestore = pendingRestoreRef.current
    if (!pendingRestore) {
      return
    }
    applyRestoreTarget(scrollElement, pendingRestore.target)
    liveScrollTopRef.current = scrollElement.scrollTop
    pendingRestoreRef.current = scrollRestoreAfterAttempt(pendingRestore, scrollElement)
  }, [entryCount, path, paneId, savedScrollTop, scrollContainerRef, setScrollPosition])

  return useCallback((position: number) => {
    liveScrollTopRef.current = position
  }, [])
}
