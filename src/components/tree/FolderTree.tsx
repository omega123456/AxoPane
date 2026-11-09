import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type Key,
  type MouseEvent,
  type RefObject,
} from 'react'
import { TreeNode, type TreeRowActions } from './TreeNode'
import { TrashTreeRow } from './TrashTreeRow'
import { TreeFoldRow } from './TreeFoldRow'
import { FavouritesPanel } from './FavouritesPanel'
import { buildContextMenuContent } from '@/components/menus/menu-definitions'
import { detectPlatformOs, isWindowsPlatform } from '@/lib/keymap'
import { TRASH_PATH } from '@/lib/trash'
import { buildTreeFlatModel, treeRowHeight, type TreeFlatRow } from '@/lib/tree-flat'
import { buildStickyChain, capStickyChain } from '@/lib/tree-sticky'
import { useElementVirtualizer } from '@/lib/use-element-virtualizer'
import { useLayoutStore } from '@/stores/layout-store'
import {
  navigateActivePane,
  openTabInActivePane,
  usePanesStore,
  type TreeNodeState,
} from '@/stores/panes-store'
import { useContextMenuStore } from '@/stores/context-menu-store'
import { SCROLLBAR_THIN } from '@/lib/class-names'

const PINNED_ROW_BASE_Z_INDEX = 10

function rowKey(row: TreeFlatRow): string {
  return row.renderKey
}

function openTreeContextMenu(title: string, path: string, event: MouseEvent) {
  event.preventDefault()
  const paneId = usePanesStore.getState().activePaneId
  useContextMenuStore.getState().openMenu({
    paneId,
    title,
    chip: 'DIR',
    x: event.clientX,
    y: event.clientY,
    ...buildContextMenuContent(paneId, { kind: 'tree', path }, detectPlatformOs()),
  })
}

const treeRowActions: TreeRowActions = {
  onToggle: (path) => void usePanesStore.getState().toggleTreeNode(path),
  onNavigate: navigateActivePane,
  onOpenTab: openTabInActivePane,
  onContextMenu: (node, event) => openTreeContextMenu(node.name, node.path, event),
}

const trashRowActions = {
  onNavigate: () => navigateActivePane(TRASH_PATH),
  onOpenTab: () => openTabInActivePane(TRASH_PATH),
  onContextMenu: (event: MouseEvent) => {
    const trashLabel = isWindowsPlatform() ? 'Recycle Bin' : 'Trash'
    openTreeContextMenu(trashLabel, TRASH_PATH, event)
  },
}

const ROW_LAYER_CLASS_NAME =
  'absolute inset-x-0 overflow-hidden bg-light-base dark:bg-dark-base mica:bg-transparent'

type PinnedRow = { path: string; chainIndex: number; top: number }

function useTreeModel(activePath: string) {
  const volumes = usePanesStore((state) => state.volumes)
  const treeNodes = usePanesStore((state) => state.treeNodes)
  const treeFold = usePanesStore((state) => state.treeFold)
  const stickyChain = useMemo(
    () => buildStickyChain(treeNodes, activePath),
    [treeNodes, activePath],
  )
  const platform = detectPlatformOs()
  const fold = useMemo(
    () => ({
      chain: stickyChain,
      revealedSiblings: new Set(treeFold.revealedSiblings),
      showHiddenLevels: treeFold.showHiddenLevels,
    }),
    [stickyChain, treeFold],
  )
  const model = useMemo(
    () => buildTreeFlatModel(treeNodes, volumes, { fold, platform }),
    [treeNodes, volumes, fold, platform],
  )
  const pinnedChain = useMemo(
    () => capStickyChain(stickyChain, model.indexByPath),
    [stickyChain, model.indexByPath],
  )
  return { ...model, treeNodes, pinnedChain, isMacOs: platform === 'macos' }
}

function useTreeVirtualRows(
  rows: TreeFlatRow[],
  offsets: number[],
  scrollRef: RefObject<HTMLDivElement | null>,
) {
  const rowVirtualizer = useElementVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: (index) => treeRowHeight(rows[index]),
    overscan: 10,
  })
  const virtualItems = rowVirtualizer.getVirtualItems()
  const itemsToRender =
    virtualItems.length > 0
      ? virtualItems
      : rows.map((_, index) => ({ key: index, index, start: offsets[index] }))
  const totalHeight = virtualItems.length > 0 ? rowVirtualizer.getTotalSize() : offsets[rows.length]
  return { rowVirtualizer, itemsToRender, totalHeight }
}

function useRevealActiveFolder(
  activePath: string,
  indexByPath: Map<string, number>,
  scrollToIndex: (index: number) => void,
) {
  const revealPath = usePanesStore((state) => state.revealPath)
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
      scrollToIndex(activeIndex)
    }
  }, [activeIndex, scrollToIndex])
}

type PinnedRowLayout = {
  pinnedChain: string[]
  indexByPath: Map<string, number>
  offsets: number[]
  rows: TreeFlatRow[]
}

function pinnedRowsAt(layout: PinnedRowLayout, scrollTop: number): PinnedRow[] {
  const { pinnedChain, indexByPath, offsets, rows } = layout
  const pinned: PinnedRow[] = []
  let slotTop = 0
  pinnedChain.forEach((path, chainIndex) => {
    const index = indexByPath.get(path)
    if (index === undefined) {
      return
    }
    if (offsets[index] - scrollTop < slotTop) {
      pinned.push({ path, chainIndex, top: slotTop })
    }
    slotTop += treeRowHeight(rows[index])
  })
  return pinned
}

type TreeRowContentProps = {
  row: TreeFlatRow
  treeNodes: Record<string, TreeNodeState>
  activePath: string
}

function TreeRowContent({ row, treeNodes, activePath }: TreeRowContentProps) {
  if (row.kind === 'header') {
    return (
      <div className="flex h-tree-header items-end px-3 pb-1">
        <span className="text-caption text-light-text-tertiary dark:text-dark-text-tertiary">
          {row.label}
        </span>
      </div>
    )
  }
  if (row.kind === 'fold-siblings') {
    return (
      <TreeFoldRow
        kind="siblings"
        count={row.count}
        depth={row.depth}
        onReveal={() => usePanesStore.getState().revealTreeSiblings(row.parentPath)}
      />
    )
  }
  if (row.kind === 'fold-levels') {
    return (
      <TreeFoldRow
        kind="levels"
        hiddenNames={row.hiddenPaths.map((path) => treeNodes[path]?.name ?? path)}
        depth={row.depth}
        onReveal={() => usePanesStore.getState().revealTreeHiddenLevels()}
      />
    )
  }
  if (row.kind === 'trash') {
    return (
      <TrashTreeRow
        isCurrent={activePath === TRASH_PATH}
        onNavigate={trashRowActions.onNavigate}
        onOpenTab={trashRowActions.onOpenTab}
        onContextMenu={trashRowActions.onContextMenu}
      />
    )
  }
  const node = treeNodes[row.path]
  return (
    <TreeNode
      node={node}
      depth={row.depth}
      volume={row.volume}
      isCurrent={activePath === node.path}
      actions={treeRowActions}
    />
  )
}

function isMissingNode(row: TreeFlatRow, treeNodes: Record<string, TreeNodeState>) {
  return row.kind === 'node' && !treeNodes[row.path]
}

type PinnedAncestorsProps = {
  pinnedRows: PinnedRow[]
  rows: TreeFlatRow[]
  indexByPath: Map<string, number>
  treeNodes: Record<string, TreeNodeState>
  activePath: string
  isMacOs: boolean
}

function pinnedOverlayRightInsetClass(isMacOs: boolean) {
  return isMacOs ? 'right-3' : 'right-1'
}

function PinnedAncestors({
  pinnedRows,
  rows,
  indexByPath,
  treeNodes,
  activePath,
  isMacOs,
}: PinnedAncestorsProps) {
  return (
    <div
      aria-hidden="true"
      inert
      className={`pointer-events-none absolute left-1 top-0 ${pinnedOverlayRightInsetClass(isMacOs)}`}
    >
      {pinnedRows.map(({ path, chainIndex, top }) => {
        const node = treeNodes[path]
        const index = indexByPath.get(path)
        if (!node) {
          return null
        }
        if (index === undefined) {
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
              zIndex: PINNED_ROW_BASE_Z_INDEX + chainIndex,
            }}
          >
            <TreeNode
              node={node}
              depth={row.kind === 'node' ? row.depth : 0}
              volume={row.kind === 'node' ? row.volume : undefined}
              isCurrent={activePath === node.path}
              isPinned
              actions={treeRowActions}
            />
          </div>
        )
      })}
    </div>
  )
}

type TreeRowsProps = {
  items: { key: Key; index: number; start: number }[]
  rows: TreeFlatRow[]
  treeNodes: Record<string, TreeNodeState>
  activePath: string
  totalHeight: number
}

function TreeRows({ items, rows, treeNodes, activePath, totalHeight }: TreeRowsProps) {
  return (
    <div
      role="tree"
      aria-label="Folders"
      style={{ height: `${totalHeight}px`, position: 'relative' }}
      className="mx-1 min-h-full"
    >
      {items.map((item) => {
        const row = rows[item.index]
        if (isMissingNode(row, treeNodes)) {
          return null
        }
        return (
          <div
            key={rowKey(row)}
            className={ROW_LAYER_CLASS_NAME}
            style={{ top: `${item.start}px` }}
          >
            <TreeRowContent row={row} treeNodes={treeNodes} activePath={activePath} />
          </div>
        )
      })}
    </div>
  )
}

export function FolderTree() {
  const activePaneId = usePanesStore((state) => state.activePaneId)
  const activePath = usePanesStore((state) => state.panes[activePaneId].path)
  const treeWidthPx = useLayoutStore((state) => state.treeWidthPx)
  const scrollRef = useRef<HTMLDivElement | null>(null)
  const [scrollTop, setScrollTop] = useState(0)
  const { rows, offsets, indexByPath, treeNodes, pinnedChain, isMacOs } = useTreeModel(activePath)
  const { rowVirtualizer, itemsToRender, totalHeight } = useTreeVirtualRows(
    rows,
    offsets,
    scrollRef,
  )
  const scrollToIndex = useCallback(
    (index: number) => rowVirtualizer.scrollToIndex(index),
    [rowVirtualizer],
  )
  useRevealActiveFolder(activePath, indexByPath, scrollToIndex)
  const pinnedRows = useMemo(
    () => pinnedRowsAt({ pinnedChain, indexByPath, offsets, rows }, scrollTop),
    [pinnedChain, indexByPath, offsets, rows, scrollTop],
  )

  return (
    <aside
      style={{ width: `${treeWidthPx}px` }}
      className="relative flex min-h-0 shrink-0 flex-col bg-light-base dark:bg-dark-base mica:bg-transparent"
    >
      <FavouritesPanel />
      <div className="mx-3 h-px flex-none bg-light-divider dark:bg-dark-divider" />
      <div className="relative flex min-h-0 flex-1 flex-col">
        <div
          ref={scrollRef}
          data-testid="folder-tree-scroll"
          className={`min-h-0 flex-1 overflow-x-hidden overflow-y-auto overscroll-contain pb-2 ${SCROLLBAR_THIN}`}
          onScroll={(event) => setScrollTop(event.currentTarget.scrollTop)}
        >
          <TreeRows
            items={itemsToRender}
            rows={rows}
            treeNodes={treeNodes}
            activePath={activePath}
            totalHeight={totalHeight}
          />
        </div>
        <PinnedAncestors
          pinnedRows={pinnedRows}
          rows={rows}
          indexByPath={indexByPath}
          treeNodes={treeNodes}
          activePath={activePath}
          isMacOs={isMacOs}
        />
      </div>
    </aside>
  )
}
