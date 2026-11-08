import type { MouseEvent } from 'react'
import type { PaneId } from '@/types/pane'
import { BreadcrumbBar } from './BreadcrumbBar'
import { FilePaneDetails, FilePaneGraphical } from './FilePaneViews'
import {
  useFilePaneKeyboard,
  usePaneBodyState,
  usePaneContext,
  usePaneDataRequests,
  usePaneMarquee,
  usePaneRefs,
  usePaneRowInteractions,
  usePaneShellFocus,
  useSharedViewProps,
} from './use-file-pane'
import { PARENT_ROW_INDEX } from './use-pane-focus'
import { usePointerActivation } from './use-pointer-activation'

type FilePaneProps = {
  paneId: PaneId
}

export function FilePane({ paneId }: FilePaneProps) {
  const { context, activeTabId } = usePaneContext(paneId)
  const { pane, viewMode, layout } = context
  const refs = usePaneRefs()
  const { paneRef, scrollContainerRef, detailsViewRef, graphicalViewRef } = refs
  const { setActivePane, focusPaneShell } = usePaneShellFocus(paneId, paneRef)
  const requests = usePaneDataRequests(context, activeTabId, refs)
  const pointer = usePointerActivation(paneId)
  const keyboard = useFilePaneKeyboard(context, refs, (entryId) => {
    focusPaneShell()
    pointer.activateEntry(entryId)
  })
  const marquee = usePaneMarquee(context, refs, focusPaneShell)
  const row = usePaneRowInteractions({
    context,
    focusPaneShell,
    activateEntryFromPointer: pointer.activateEntryFromPointer,
  })
  const shared = useSharedViewProps({ context, requests, marquee, row })
  const bodyState = usePaneBodyState(context)
  const showMenuOnEmptyArea = (event: MouseEvent) => {
    if (event.target === scrollContainerRef.current) row.showMenu(event)
  }

  return (
    <section
      ref={paneRef}
      data-pane-id={paneId}
      aria-label={pane.title}
      className="relative flex h-full min-h-0 min-w-0 flex-col overflow-hidden border border-light-stroke bg-light-layer outline-none dark:border-dark-stroke dark:bg-dark-layer"
      onMouseDown={() => setActivePane(paneId)}
      onKeyDown={(event) => keyboard.handlePaneKeyDown(event, false)}
      tabIndex={0}
      onContextMenu={showMenuOnEmptyArea}
    >
      <BreadcrumbBar pane={pane} />
      {viewMode === 'details' ? (
        <FilePaneDetails
          shared={shared}
          bodyState={bodyState}
          viewRef={detailsViewRef}
          scrollContainerRef={scrollContainerRef}
          hasParent={layout.hasParent}
          usesDetachedMacScrollbars={context.platform === 'macos'}
          onParentFocus={() => keyboard.focusByRowIndex(PARENT_ROW_INDEX)}
          onParentActivate={pointer.activateParentFromPointer}
        />
      ) : (
        <FilePaneGraphical
          shared={shared}
          bodyState={bodyState}
          viewRef={graphicalViewRef}
          scrollContainerRef={scrollContainerRef}
          mode={viewMode}
          onThumbnailRangeChange={requests.handleThumbnailRangeChange}
          onFocusEntry={row.selectSingleEntry}
        />
      )}
    </section>
  )
}
