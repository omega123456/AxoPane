import { useEffect, useLayoutEffect, useMemo, useRef, useState, type MouseEvent } from 'react'
import { TreeNode, type TreeRowActions } from './TreeNode'
import { TrashTreeRow } from './TrashTreeRow'
import { buildContextMenuContent } from '@/components/menus/menu-definitions'
import { detectPlatformOs } from '@/lib/keymap'
import { TRASH_PATH } from '@/lib/trash'
import { buildTreeFlatModel, treeRowHeight, TREE_ROW_HEIGHT_PX, type TreeFlatRow } from '@/lib/tree-flat'
import { buildStickyChain } from '@/lib/tree-sticky'
import { useElementVirtualizer } from '@/lib/use-element-virtualizer'
import { useLayoutStore } from '@/stores/layout-store'
import { usePanesStore } from '@/stores/panes-store'
import { useContextMenuStore } from '@/stores/context-menu-store'

function rowKey(row: TreeFlatRow): string {
  return row.renderKey
}

export function FolderTree() {
  const volumes = usePanesStore((state) => state.volumes)
  const revealPath = usePanesStore((state) => state.revealPath)
  const activePaneId = usePanesStore((state) => state.activePaneId)
  const activePath = usePanesStore((state) => state.panes[activePaneId].path)
  const treeNodes = usePanesStore((state) => state.treeNodes)
  const toggleTreeNode = usePanesStore((state) => state.toggleTreeNode)
  const navigatePane = usePanesStore((state) => state.navigatePane)
  const openTabFromPath = usePanesStore((state) => state.openTabFromPath)
  const openMenu = useContextMenuStore((state) => state.openMenu)
  const treeWidthPx = useLayoutStore((state) => state.treeWidthPx)

  const scrollRef = useRef<HTMLDivElement | null>(null)
  // Live scroll offset, held in component state (not just a ref) because the
  // pinned-ancestor overlay below must re-render as it changes to decide which
  // ancestors are pinned. Only the ~visible window of rows is mounted, so this
  // per-frame re-render stays cheap - unlike the main pane, which keeps scroll
  // in a ref precisely to avoid re-rendering its (unvirtualized-key) rows.
  const [scrollTop, setScrollTop] = useState(0)

  // One flat, virtualization-ready model spanning every category, volume subtree
  // and the trash row. Rebuilt only when the underlying tree/volumes change.
  const { rows, offsets, indexByPath } = useMemo(
    () => buildTreeFlatModel(treeNodes, volumes),
    [treeNodes, volumes],
  )
  const isMacOs = detectPlatformOs() === 'macos'
  const rowLayerClassName = 'absolute inset-x-0 overflow-hidden bg-light-tree dark:bg-dark-tree'

  // The ancestor chain (volume root -> active node) pins to the top of the
  // scroll area as the user scrolls past each ancestor - an editor-style "sticky
  // scroll" breadcrumb for the active folder's lineage, reimplemented as a
  // manual overlay (below) instead of per-row `position: sticky`.
  const stickyChain = useMemo(() => buildStickyChain(treeNodes, activePath), [treeNodes, activePath])

  const rowVirtualizer = useElementVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: (index) => treeRowHeight(rows[index]),
    overscan: 10,
  })
  const virtualItems = rowVirtualizer.getVirtualItems()
  // jsdom (unit tests) has no layout, so the virtualizer reports no items -
  // fall back to rendering every row, matching the main pane. Real runs always
  // take the virtualized path.
  const itemsToRender =
    virtualItems.length > 0
      ? virtualItems
      : rows.map((_, index) => ({ key: index, index, start: offsets[index] }))
  const totalHeight = virtualItems.length > 0 ? rowVirtualizer.getTotalSize() : offsets[rows.length]

  // Track the active folder: expand its ancestor chain so its row exists in the
  // model, then scroll that row into view once it does.
  useEffect(() => {
    void revealPath(activePath)
  }, [activePath, revealPath])

  const activeIndex = useMemo(() => {
    if (activePath === TRASH_PATH) {
      return indexByPath.get(TRASH_PATH)
    }
    return activePath ? indexByPath.get(activePath) : undefined
  }, [activePath, indexByPath])

  useEffect(() => {
    if (activeIndex !== undefined) {
      rowVirtualizer.scrollToIndex(activeIndex)
    }
  }, [activeIndex, rowVirtualizer])

  // `TreeNode`/`TrashTreeRow` are memoized; a single stable handler object per
  // render is what lets them skip re-rendering. Each handler closes over current
  // store values, so they are refreshed into `latestRef` after every commit and
  // the stable dispatchers below read through it - the same pattern as the pane.
  const latest = { activePaneId, toggleTreeNode, navigatePane, openTabFromPath, openMenu }
  const latestRef = useRef(latest)
  useLayoutEffect(() => {
    latestRef.current = latest
  })

  const actions = useMemo<TreeRowActions>(
    () => ({
      onToggle: (path) => void latestRef.current.toggleTreeNode(path),
      onNavigate: (path) => void latestRef.current.navigatePane(latestRef.current.activePaneId, path),
      onOpenTab: (path) => void latestRef.current.openTabFromPath(latestRef.current.activePaneId, path),
      onContextMenu: (node, event) => {
        event.preventDefault()
        const paneId = latestRef.current.activePaneId
        latestRef.current.openMenu({
          paneId,
          title: node.name,
          chip: 'DIR',
          x: event.clientX,
          y: event.clientY,
          ...buildContextMenuContent(paneId, { kind: 'tree', path: node.path }, detectPlatformOs()),
        })
      },
    }),
    [],
  )

  const trashActions = useMemo(
    () => ({
      onNavigate: () => void latestRef.current.navigatePane(latestRef.current.activePaneId, TRASH_PATH),
      onOpenTab: () => void latestRef.current.openTabFromPath(latestRef.current.activePaneId, TRASH_PATH),
      onContextMenu: (event: MouseEvent) => {
        event.preventDefault()
        const paneId = latestRef.current.activePaneId
        const trashLabel = detectPlatformOs() === 'windows' ? 'Recycle Bin' : 'Trash'
        latestRef.current.openMenu({
          paneId,
          title: trashLabel,
          chip: 'DIR',
          x: event.clientX,
          y: event.clientY,
          ...buildContextMenuContent(paneId, { kind: 'tree', path: TRASH_PATH }, detectPlatformOs()),
        })
      },
    }),
    [],
  )

  // Ancestors scrolled up past their stacked slot are drawn in an opaque overlay
  // pinned to the top; ancestors still in their natural position render normally
  // in the window below. An ancestor's slot is `chainIndex` row-heights down from
  // the viewport top; it pins once its natural top would sit above that slot.
  //
  // `top` is the slot's position in *viewport* space (measured from the scroll
  // area's top edge), because the overlay is rendered outside the scroll
  // container - see the render below. Membership still keys off `scrollTop`, but
  // the position itself carries no `scrollTop` term, so it can never lag the
  // native scroll and rubber-band.
  const pinnedRows = useMemo(() => {
    const pinned: { path: string; chainIndex: number; top: number }[] = []
    stickyChain.forEach((path, chainIndex) => {
      const index = indexByPath.get(path)
      if (index === undefined) {
        return
      }
      const slotTop = chainIndex * TREE_ROW_HEIGHT_PX
      if (offsets[index] - scrollTop < slotTop) {
        pinned.push({ path, chainIndex, top: slotTop })
      }
    })
    return pinned
  }, [stickyChain, indexByPath, offsets, scrollTop])

  return (
    <aside
      // Width is user-draggable runtime geometry; no static token can express it.
      style={{ width: `${treeWidthPx}px` }}
      className="relative flex min-h-0 shrink-0 flex-col border-r border-light-border bg-light-tree dark:border-dark-border dark:bg-dark-tree"
    >
      <div
        ref={scrollRef}
        data-testid="folder-tree-scroll"
        className="min-h-0 flex-1 overflow-x-hidden overflow-y-auto overscroll-contain pb-2 scrollbar-thin scrollbar-track-transparent scrollbar-thumb-light-text-faint dark:scrollbar-thumb-dark-text-faint"
        onScroll={(event) => setScrollTop(event.currentTarget.scrollTop)}
      >
        {/*
          Styling-constraint exception: runtime geometry only. The total scroll
          height and each row's top offset come from @tanstack/react-virtual and
          are continuous px values no static token can express. Every
          design-system value elsewhere stays a pure Tailwind utility.
        */}
        <div style={{ height: `${totalHeight}px`, position: 'relative' }} className="min-h-full">
          {itemsToRender.map((item) => {
            const row = rows[item.index]
            const style = {
              top: `${item.start}px`,
            }

            if (row.kind === 'header') {
              return (
                <div key={rowKey(row)} className={rowLayerClassName} style={style}>
                  <div className="flex h-tree-header items-end px-3 pb-1">
                    <span className="font-mono text-uxs uppercase tracking-wide text-light-text-faint dark:text-dark-text-faint">
                      {row.label}
                    </span>
                  </div>
                </div>
              )
            }

            if (row.kind === 'trash') {
              return (
                <div key={rowKey(row)} className={rowLayerClassName} style={style}>
                  <TrashTreeRow
                    isCurrent={activePath === TRASH_PATH}
                    onNavigate={trashActions.onNavigate}
                    onOpenTab={trashActions.onOpenTab}
                    onContextMenu={trashActions.onContextMenu}
                  />
                </div>
              )
            }

            const node = treeNodes[row.path]
            if (!node) {
              return null
            }
            return (
              <div key={rowKey(row)} className={rowLayerClassName} style={style}>
                <TreeNode
                  node={node}
                  depth={row.depth}
                  volume={row.volume}
                  isCurrent={activePath === node.path}
                  actions={actions}
                />
              </div>
            )
          })}
        </div>
      </div>

      {/*
        Pinned-ancestor overlay. It lives OUTSIDE the scroll container so it never
        inherits the container's scroll offset: its rows sit in viewport space at a
        fixed `top`. When this overlay was a child of the scrolling content, the
        native scroll moved it every frame while a React `scrollTop` state term
        tried to hold it in place - the two were a frame out of sync, which read as
        a rubber-band jitter. The wrapper is zero-height so it never intercepts
        pointer events; each pinned row re-enables them for itself.
      */}
      <div
        className={`pointer-events-none absolute left-0 top-0 ${isMacOs ? 'right-2' : 'right-0'}`}
      >
        {pinnedRows.map(({ path, chainIndex, top }) => {
          const node = treeNodes[path]
          const index = indexByPath.get(path)
          if (!node || index === undefined) {
            return null
          }
          const row = rows[index]
          return (
            <div
              key={`pinned-${path}`}
              data-testid="tree-pinned-row"
              className="pointer-events-auto absolute inset-x-0"
              style={{
                top: `${top}px`,
                zIndex: 10 + chainIndex,
              }}
            >
              <TreeNode
                node={node}
                depth={row.kind === 'node' ? row.depth : 0}
                volume={row.kind === 'node' ? row.volume : undefined}
                isCurrent={activePath === node.path}
                isPinned
                actions={actions}
              />
            </div>
          )
        })}
      </div>
    </aside>
  )
}
