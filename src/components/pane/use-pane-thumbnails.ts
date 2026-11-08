import { useCallback, useEffect, useRef } from 'react'
import {
  thumbnailCandidatesInRange,
  type GraphicalThumbnailRange,
} from '@/lib/thumbnail-candidates'
import type { PaneViewMode } from '@/lib/pane-view'
import type { DirectoryEntry } from '@/lib/types/ipc'
import { useThumbnailStore } from '@/stores/thumbnail-store'
import type { PaneId } from '@/types/pane'

type ThumbnailScope = {
  paneId: PaneId
  tabId: string
  path: string
  mode: PaneViewMode
}

type PendingThumbnailRange = {
  scope: ThumbnailScope
  range: GraphicalThumbnailRange
  entries: DirectoryEntry[]
}

type PaneThumbnailOptions = ThumbnailScope & {
  entries: DirectoryEntry[]
}

function requestPendingThumbnails(pending: PendingThumbnailRange | null) {
  if (!pending || pending.scope.mode !== 'thumbnails') return
  const candidates = thumbnailCandidatesInRange(pending.entries, pending.range)
  void useThumbnailStore
    .getState()
    .setVisibleCandidates(pending.scope, candidates)
    .catch(() => undefined)
}

function useCancelThumbnailScopes({ paneId, tabId, path, mode }: ThumbnailScope) {
  const cancelThumbnailScope = useThumbnailStore((state) => state.cancelScope)
  const activeTabIdRef = useRef(tabId)
  const thumbnailPathRef = useRef(path)

  useEffect(() => {
    const previousTabId = activeTabIdRef.current
    const previousPath = thumbnailPathRef.current
    activeTabIdRef.current = tabId
    thumbnailPathRef.current = path
    const leftThumbnailScope = mode !== 'thumbnails' || previousPath !== path
    if (leftThumbnailScope) void cancelThumbnailScope(paneId, tabId)
    if (previousTabId !== tabId) void cancelThumbnailScope(paneId, previousTabId)
  }, [tabId, cancelThumbnailScope, path, paneId, mode])

  useEffect(
    () => () => {
      void cancelThumbnailScope(paneId, activeTabIdRef.current)
    },
    [cancelThumbnailScope, paneId],
  )
}

export function usePaneThumbnails(options: PaneThumbnailOptions) {
  const { paneId, tabId, path, mode, entries } = options
  const thumbnailRequestFrameRef = useRef<number | undefined>(undefined)
  const pendingThumbnailRangeRef = useRef<PendingThumbnailRange | null>(null)
  useCancelThumbnailScopes({ paneId, tabId, path, mode })

  useEffect(
    () => () => {
      if (thumbnailRequestFrameRef.current !== undefined)
        window.cancelAnimationFrame(thumbnailRequestFrameRef.current)
    },
    [],
  )

  return useCallback(
    (range: GraphicalThumbnailRange) => {
      pendingThumbnailRangeRef.current = { scope: { paneId, tabId, path, mode }, range, entries }
      if (thumbnailRequestFrameRef.current !== undefined) return
      thumbnailRequestFrameRef.current = window.requestAnimationFrame(() => {
        thumbnailRequestFrameRef.current = undefined
        requestPendingThumbnails(pendingThumbnailRangeRef.current)
      })
    },
    [tabId, entries, path, paneId, mode],
  )
}
