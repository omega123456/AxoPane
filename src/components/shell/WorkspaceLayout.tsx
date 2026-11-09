import { useRef, type ReactNode, type RefObject } from 'react'
import { FilePane } from '@/components/pane/FilePane'
import { FolderTree } from '@/components/tree/FolderTree'
import { ResizeHandle } from '@/components/shell/ResizeHandle'
import { TabDragDropProvider } from '@/components/pane/TabDragDropProvider'
import { TabBar } from '@/components/pane/TabBar'
import { commitLayout, useDragResize } from '@/components/shell/use-drag-resize'
import {
  PANE_SPLIT_MAX,
  PANE_SPLIT_MIN,
  TREE_WIDTH_MAX,
  TREE_WIDTH_MIN,
  useLayoutStore,
} from '@/stores/layout-store'
import { usePanesStore } from '@/stores/panes-store'
import type { PaneId } from '@/types/pane'
import { fractionToPercent } from '@/lib/numbers'

const TREE_STEP_PX = 16
const SPLIT_STEP = 0.02

function TreeResizeHandle() {
  const treeWidthPx = useLayoutStore((state) => state.treeWidthPx)
  const setTreeWidthPx = useLayoutStore((state) => state.setTreeWidthPx)
  const resizeCallbacks = useDragResize({
    value: treeWidthPx,
    stepSize: TREE_STEP_PX,
    setValue: setTreeWidthPx,
  })
  return (
    <ResizeHandle
      ariaLabel="Resize folder tree"
      value={treeWidthPx}
      min={TREE_WIDTH_MIN}
      max={TREE_WIDTH_MAX}
      showRestLine={false}
      {...resizeCallbacks}
    />
  )
}

function PaneSplitDivider({ panesRef }: { panesRef: RefObject<HTMLDivElement | null> }) {
  const paneSplit = useLayoutStore((state) => state.paneSplit)
  const setPaneSplit = useLayoutStore((state) => state.setPaneSplit)
  const dragStartSplit = useRef(paneSplit)

  function resizeSplit(deltaX: number) {
    const width = panesRef.current?.getBoundingClientRect().width
    if (!width) {
      return
    }
    setPaneSplit(dragStartSplit.current + deltaX / width)
  }

  return (
    <div
      className="pointer-events-none absolute inset-y-0 z-20"
      style={{
        left: `calc((100% - var(--spacing-gutter)) * ${paneSplit} + var(--spacing-gutter) / 2)`,
      }}
    >
      <div className="pointer-events-auto h-full">
        <ResizeHandle
          ariaLabel="Resize panes"
          value={fractionToPercent(paneSplit)}
          min={fractionToPercent(PANE_SPLIT_MIN)}
          max={fractionToPercent(PANE_SPLIT_MAX)}
          showRestLine={false}
          onDragStart={() => {
            dragStartSplit.current = paneSplit
          }}
          onResize={resizeSplit}
          onStep={(delta) => setPaneSplit(paneSplit + delta * SPLIT_STEP)}
          onCommit={commitLayout}
        />
      </div>
    </div>
  )
}

function PaneCell({ paneId, showsActiveRule }: { paneId: PaneId; showsActiveRule: boolean }) {
  return (
    <div className="relative min-h-0 min-w-0">
      <FilePane paneId={paneId} />

      {showsActiveRule ? (
        <span
          aria-hidden="true"
          data-testid="active-pane-rule"
          className="pointer-events-none absolute inset-x-0 top-0 z-10 h-0.5 bg-light-accent dark:bg-dark-accent"
        />
      ) : null}
    </div>
  )
}

function splitColumnStyle(paneCount: number, paneSplit: number) {
  return {
    gridTemplateColumns:
      paneCount === 1
        ? 'minmax(0, 1fr)'
        : `minmax(0, ${paneSplit}fr) minmax(0, ${1 - paneSplit}fr)`,
  }
}

export function WorkspaceLayout({ commandBar }: { commandBar?: ReactNode }) {
  const paneSplit = useLayoutStore((state) => state.paneSplit)
  const defaultPaneMode = useLayoutStore((state) => state.defaultPaneMode)
  const activePaneId = usePanesStore((state) => state.activePaneId)
  const panesRef = useRef<HTMLDivElement>(null)
  const paneIds: PaneId[] = defaultPaneMode === 'single' ? [activePaneId] : ['left', 'right']
  const isDual = paneIds.length > 1
  const columnStyle = splitColumnStyle(paneIds.length, paneSplit)

  return (
    <div className="flex min-h-0 min-w-0 w-full flex-1 overflow-hidden">
      <FolderTree />
      <TreeResizeHandle />
      <TabDragDropProvider>
        <div className="flex min-h-0 min-w-0 flex-1 flex-col px-gutter">
          <div className="relative grid h-tabs shrink-0 gap-x-gutter" style={columnStyle}>
            <span
              aria-hidden="true"
              className="absolute inset-x-0 bottom-0 h-px bg-light-stroke dark:bg-dark-stroke"
            />
            {paneIds.map((paneId) => (
              <PaneTabs key={paneId} paneId={paneId} />
            ))}
          </div>
          {commandBar}
          <div
            ref={panesRef}
            className="relative mt-gutter grid min-h-0 flex-1 grid-rows-1 gap-x-gutter"
            style={columnStyle}
          >
            {paneIds.map((paneId) => (
              <PaneCell
                key={paneId}
                paneId={paneId}
                showsActiveRule={paneId === activePaneId && isDual}
              />
            ))}
            {isDual ? <PaneSplitDivider panesRef={panesRef} /> : null}
          </div>
        </div>
      </TabDragDropProvider>
    </div>
  )
}

function PaneTabs({ paneId }: { paneId: PaneId }) {
  const title = usePanesStore((state) => state.panes[paneId].title)
  const path = usePanesStore((state) => state.panes[paneId].path)
  return <TabBar paneId={paneId} title={title} currentPath={path} />
}
