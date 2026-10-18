import { useRef, type ReactNode } from 'react'
import { FilePane } from '@/components/pane/FilePane'
import { FolderTree } from '@/components/tree/FolderTree'
import { ResizeHandle } from '@/components/shell/ResizeHandle'
import { TabDragDropProvider } from '@/components/pane/TabDragDropProvider'
import { TabBar } from '@/components/pane/TabBar'
import { persistAppConfig } from '@/lib/app-config'
import {
  PANE_SPLIT_MAX,
  PANE_SPLIT_MIN,
  TREE_WIDTH_MAX,
  TREE_WIDTH_MIN,
  useLayoutStore,
} from '@/stores/layout-store'
import { usePanesStore } from '@/stores/panes-store'
import type { PaneId } from '@/types/pane'

// Keyboard nudge granularity for each divider.
const TREE_STEP_PX = 16
const SPLIT_STEP = 0.02

/**
 * The resizable workspace: the full-height folder tree, a draggable divider,
 * then each pane's tabs, the shared command bar and the pane cards. In dual
 * mode a second divider splits the two panes. Both sizes are stored in the
 * layout store and persisted so they survive a restart.
 */
export function WorkspaceLayout({ commandBar }: { commandBar?: ReactNode }) {
  const treeWidthPx = useLayoutStore((state) => state.treeWidthPx)
  const setTreeWidthPx = useLayoutStore((state) => state.setTreeWidthPx)
  const paneSplit = useLayoutStore((state) => state.paneSplit)
  const setPaneSplit = useLayoutStore((state) => state.setPaneSplit)
  const defaultPaneMode = useLayoutStore((state) => state.defaultPaneMode)
  const activePaneId = usePanesStore((state) => state.activePaneId)

  const panesRef = useRef<HTMLDivElement>(null)
  // Sizes captured when a drag starts, so movement is applied relative to the
  // grab point and the divider never jumps to the cursor.
  const dragStartTree = useRef(treeWidthPx)
  const dragStartSplit = useRef(paneSplit)

  function commit() {
    void persistAppConfig()
  }

  function resizeTree(deltaX: number) {
    setTreeWidthPx(dragStartTree.current + deltaX)
  }

  function resizeSplit(deltaX: number) {
    const width = panesRef.current?.getBoundingClientRect().width
    if (!width) {
      return
    }
    setPaneSplit(dragStartSplit.current + deltaX / width)
  }

  const paneIds: PaneId[] = defaultPaneMode === 'single' ? [activePaneId] : ['left', 'right']
  // Styling-constraint exception: runtime geometry only. The user-dragged split
  // ratio is continuous, so the tab row and the pane row share it as an inline
  // column template; every other value is a token utility.
  const columnStyle = {
    gridTemplateColumns:
      paneIds.length === 1
        ? 'minmax(0, 1fr)'
        : `minmax(0, ${paneSplit}fr) minmax(0, ${1 - paneSplit}fr)`,
  }

  return (
    <div className="flex min-h-0 min-w-0 w-full flex-1 overflow-hidden">
      <FolderTree />
      <ResizeHandle
        ariaLabel="Resize folder tree"
        value={treeWidthPx}
        min={TREE_WIDTH_MIN}
        max={TREE_WIDTH_MAX}
        showRestLine={false}
        onDragStart={() => {
          dragStartTree.current = treeWidthPx
        }}
        onResize={resizeTree}
        onStep={(delta) => setTreeWidthPx(treeWidthPx + delta * TREE_STEP_PX)}
        onCommit={commit}
      />
      <TabDragDropProvider>
        <div className="flex min-h-0 min-w-0 flex-1 flex-col px-gutter">
          {/*
            Each pane's tabs sit straight on the window base. The strip's bottom
            line doubles as the command bar's top edge; each selected tab covers
            it, so tab and bar read as one shape.
          */}
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
              <div key={paneId} className="relative min-h-0 min-w-0">
                <FilePane paneId={paneId} />
                {/* The active pane's marker: a 2px accent rule on the card's top edge. */}
                {paneId === activePaneId && paneIds.length > 1 ? (
                  <span
                    aria-hidden="true"
                    data-testid="active-pane-rule"
                    className="pointer-events-none absolute inset-x-0 top-0 z-10 h-0.5 bg-light-accent dark:bg-dark-accent"
                  />
                ) : null}
              </div>
            ))}
            {paneIds.length > 1 ? (
              <div
                className="pointer-events-none absolute inset-y-0 z-20"
                // Centre the divider in the gutter between the two cards.
                style={{
                  left: `calc((100% - var(--spacing-gutter)) * ${paneSplit} + var(--spacing-gutter) / 2)`,
                }}
              >
                <div className="pointer-events-auto h-full">
                  <ResizeHandle
                    ariaLabel="Resize panes"
                    value={paneSplit * 100}
                    min={PANE_SPLIT_MIN * 100}
                    max={PANE_SPLIT_MAX * 100}
                    showRestLine={false}
                    onDragStart={() => {
                      dragStartSplit.current = paneSplit
                    }}
                    onResize={resizeSplit}
                    onStep={(delta) => setPaneSplit(paneSplit + delta * SPLIT_STEP)}
                    onCommit={commit}
                  />
                </div>
              </div>
            ) : null}
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
