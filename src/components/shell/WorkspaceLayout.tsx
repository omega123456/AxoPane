import { useRef } from 'react'
import { FilePane } from '@/components/pane/FilePane'
import { FolderTree } from '@/components/tree/FolderTree'
import { ResizeHandle } from '@/components/shell/ResizeHandle'
import { persistAppConfig } from '@/lib/app-config'
import {
  PANE_SPLIT_MAX,
  PANE_SPLIT_MIN,
  TREE_WIDTH_MAX,
  TREE_WIDTH_MIN,
  useLayoutStore,
} from '@/stores/layout-store'
import { usePanesStore } from '@/stores/panes-store'

// Keyboard nudge granularity for each divider.
const TREE_STEP_PX = 16
const SPLIT_STEP = 0.02

/**
 * The resizable workspace row: folder tree, a draggable divider, then the file
 * pane(s). In dual mode a second divider splits the two panes. Both sizes are
 * stored in the layout store and persisted so they survive a restart.
 */
export function WorkspaceLayout() {
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
      {defaultPaneMode === 'single' ? (
        <div className="min-h-0 min-w-0 flex-1 overflow-hidden">
          <FilePane paneId={activePaneId} />
        </div>
      ) : (
        <div
          ref={panesRef}
          // A grid (not flexbox) so the column split rounds identically to the
          // fixed design; `divide-x` paints the divider line between the panes.
          style={{ gridTemplateColumns: `minmax(0, ${paneSplit}fr) minmax(0, ${1 - paneSplit}fr)` }}
          className="relative grid min-h-0 min-w-0 flex-1 overflow-hidden grid-rows-1 divide-x divide-light-border dark:divide-dark-border"
        >
          <FilePane paneId="left" />
          <FilePane paneId="right" />
          <div
            className="pointer-events-none absolute inset-y-0 z-10"
            style={{ left: `${paneSplit * 100}%` }}
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
        </div>
      )}
    </div>
  )
}
