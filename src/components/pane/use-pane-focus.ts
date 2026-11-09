import { useEffect, useMemo, useRef, type RefObject } from 'react'
import type { PaneViewMode } from '@/lib/pane-view'
import type { DirectoryEntry } from '@/lib/types/ipc'
import { clamp } from '@/lib/numbers'
import { usePanesStore } from '@/stores/panes-store'
import { EMPTY_SELECTION, singleEntrySelection, useSelectionStore } from '@/stores/selection-store'
import type { PaneId, PaneState } from '@/types/pane'
import type { DetailsViewHandle } from './DetailsView'
import type { GraphicalViewHandle } from './GraphicalView'

export const PARENT_ROW_ID = '..'
export const PARENT_ROW_INDEX = 0
const FALLBACK_VISIBLE_ROWS = 1

export type PaneViewRefs = {
  detailsViewRef: RefObject<DetailsViewHandle | null>
  graphicalViewRef: RefObject<GraphicalViewHandle | null>
}

export type PaneRowLayout = {
  hasParent: boolean
  parentOffset: number
  rowCount: number
}

type RowScrollTarget = PaneViewRefs & {
  viewMode: PaneViewMode
  parentOffset: number
}

type FocusedRowInput = {
  entries: DirectoryEntry[]
  focusedEntryId: string | null
  hasParent: boolean
  parentOffset: number
}

type PaneFocusOptions = PaneViewRefs & {
  paneId: PaneId
  pane: PaneState
  layout: PaneRowLayout
  viewMode: PaneViewMode
  paneRef: RefObject<HTMLElement | null>
}

function scrollViewToRow(target: RowScrollTarget, rowIndex: number) {
  if (target.viewMode === 'details') {
    target.detailsViewRef.current?.scrollToRow(rowIndex)
    return
  }
  const entryIndex = Math.max(0, rowIndex - target.parentOffset)
  target.graphicalViewRef.current?.scrollToEntry(entryIndex)
}

function focusedRowIndexFor({ entries, focusedEntryId, hasParent, parentOffset }: FocusedRowInput) {
  const parentRowFocused = hasParent && focusedEntryId === PARENT_ROW_ID
  if (parentRowFocused) {
    return PARENT_ROW_INDEX
  }
  const entryIndex = entries.findIndex((entry) => entry.id === focusedEntryId)
  return entryIndex < 0 ? -1 : entryIndex + parentOffset
}

function useFocusOnRequest(paneId: PaneId, paneRef: RefObject<HTMLElement | null>) {
  const focusRequestId = usePanesStore((state) => state.focusRequestId)
  const focusRequestPaneId = usePanesStore((state) => state.focusRequestPaneId)
  useEffect(() => {
    const focusRequestedHere = focusRequestId > 0 && focusRequestPaneId === paneId
    if (focusRequestedHere) paneRef.current?.focus()
  }, [focusRequestId, focusRequestPaneId, paneId, paneRef])
}

function useRevealScroll(paneId: PaneId, target: RowScrollTarget, focusedRowIndex: number) {
  const revealScrollRequestId = usePanesStore((state) => state.revealScrollRequests[paneId])
  const handledRevealScrollIdRef = useRef<number | undefined>(undefined)
  const { detailsViewRef, graphicalViewRef, viewMode, parentOffset } = target
  useEffect(() => {
    const alreadyHandled =
      revealScrollRequestId === undefined ||
      revealScrollRequestId === handledRevealScrollIdRef.current
    if (alreadyHandled) return
    if (focusedRowIndex < 0) return
    handledRevealScrollIdRef.current = revealScrollRequestId
    scrollViewToRow({ detailsViewRef, graphicalViewRef, viewMode, parentOffset }, focusedRowIndex)
  }, [
    detailsViewRef,
    focusedRowIndex,
    graphicalViewRef,
    parentOffset,
    revealScrollRequestId,
    viewMode,
  ])
}

export function usePaneFocus(options: PaneFocusOptions) {
  const { paneId, pane, layout, viewMode, detailsViewRef, graphicalViewRef } = options
  const { entries, focusedEntryId } = pane
  const { hasParent, parentOffset } = layout
  const setFocusedEntry = usePanesStore((state) => state.setFocusedEntry)
  const setSelection = useSelectionStore((state) => state.setSelection)
  const focusedRowIndex = useMemo(
    () => focusedRowIndexFor({ entries, focusedEntryId, hasParent, parentOffset }),
    [entries, focusedEntryId, hasParent, parentOffset],
  )
  const scrollTarget = { detailsViewRef, graphicalViewRef, viewMode, parentOffset }
  useFocusOnRequest(paneId, options.paneRef)
  useRevealScroll(paneId, scrollTarget, focusedRowIndex)

  function visibleRowCount() {
    const handle = viewMode === 'details' ? detailsViewRef.current : graphicalViewRef.current
    return handle?.getVisibleRowCount() ?? FALLBACK_VISIBLE_ROWS
  }

  function focusParentRow() {
    setFocusedEntry(paneId, PARENT_ROW_ID)
    setSelection(paneId, EMPTY_SELECTION)
    if (viewMode === 'details') detailsViewRef.current?.scrollToRow(PARENT_ROW_INDEX)
  }

  function focusByRowIndex(nextRowIndex: number) {
    const boundedRowIndex = clamp(nextRowIndex, 0, layout.rowCount - 1)
    const isParentRow = hasParent && boundedRowIndex === PARENT_ROW_INDEX
    if (isParentRow) {
      focusParentRow()
      return
    }
    const nextEntry = entries[boundedRowIndex - parentOffset]
    if (!nextEntry) return
    setFocusedEntry(paneId, nextEntry.id)
    setSelection(paneId, singleEntrySelection(nextEntry.id))
    scrollViewToRow(scrollTarget, boundedRowIndex)
  }

  return { focusedRowIndex, visibleRowCount, focusByRowIndex }
}
