import { useEffect, useRef, useState, type CSSProperties, type DragEvent } from 'react'
import { buildContextMenuContent } from '@/components/menus/menu-definitions'
import { ChevronRightIcon } from '@/components/icons'
import { EntryIcon } from '@/components/icons/EntryIcon'
import { VolumeIcon } from '@/components/icons/VolumeIcon'
import { detectPlatformOs } from '@/lib/keymap'
import { canDropInto, performDrop, resolveDropKind } from '@/lib/drag-drop'
import type { VolumeInfo } from '@/lib/types/ipc'
import { useContextMenuStore } from '@/stores/context-menu-store'
import { useDragStore } from '@/stores/drag-store'
import { usePanesStore } from '@/stores/panes-store'

type TreeNodeProps = {
  path: string
  depth: number
  /** Set for volume roots; switches the row glyph from a folder to a drive icon. */
  volume?: VolumeInfo
  /** Ordered ancestor chain (volume root -> active node); rows on this chain
   *  pin to the top of the scroll area, stacked by their position in it. */
  stickyChain?: string[]
}

export function TreeNode({ path, depth, volume, stickyChain = [] }: TreeNodeProps) {
  const node = usePanesStore((state) => state.treeNodes[path])
  const activePaneId = usePanesStore((state) => state.activePaneId)
  const activePath = usePanesStore((state) => state.panes[activePaneId].path)
  const toggleTreeNode = usePanesStore((state) => state.toggleTreeNode)
  const navigatePane = usePanesStore((state) => state.navigatePane)
  const openTabFromPath = usePanesStore((state) => state.openTabFromPath)
  const openMenu = useContextMenuStore((state) => state.openMenu)
  const endDrag = useDragStore((state) => state.end)
  const rowRef = useRef<HTMLLIElement>(null)
  const [isDropTarget, setIsDropTarget] = useState(false)

  const isCurrent = Boolean(node) && activePath === node.path
  const chainIndex = stickyChain.indexOf(path)
  const isSticky = chainIndex !== -1

  // Keep the active folder visible: scroll its row into view within the tree
  // whenever this node becomes the current one.
  useEffect(() => {
    if (isCurrent) {
      rowRef.current?.scrollIntoView({ block: 'nearest' })
    }
  }, [isCurrent])

  if (!node) {
    return null
  }

  const rowStyle: CSSProperties = {
    // Styling-constraint exception: runtime geometry only. Indentation is a
    // function of the (unbounded) tree depth, so it cannot be a static
    // utility/token. Colors/spacing above remain pure Tailwind utilities.
    paddingLeft: `${depth * 12 + 8}px`,
  }
  if (isSticky) {
    rowStyle.position = 'sticky'
    // A calc() against the fixed --spacing-tree-row token (also the row's
    // own `h-tree-row` height below), not a JS-measured value: measuring
    // asynchronously left gaps/overlaps whenever many ancestor rows mounted
    // in the same render (e.g. jumping straight to a deeply nested path).
    rowStyle.top = `calc(var(--spacing-tree-row) * ${chainIndex})`
    rowStyle.zIndex = 10 + chainIndex
  }

  // accent-blue-soft is translucent; on a sticky row that translucency would
  // let scrolled-under rows show through, so pinned+current rows get the
  // opaque *-tree-current stand-in instead.
  const rowBackgroundClassName = isCurrent
    ? isSticky
      ? 'bg-light-tree-current text-accent-blue-light dark:bg-dark-tree-current dark:text-accent-blue'
      : 'bg-accent-blue-soft text-accent-blue-light dark:text-accent-blue'
    : isSticky
      ? 'bg-light-tree text-light-text-soft dark:bg-dark-tree dark:text-dark-text-soft'
      : 'text-light-text-soft dark:text-dark-text-soft'
  const rowHoverClassName = isSticky
    ? isCurrent
      ? 'hover:bg-light-tree-current-hover dark:hover:bg-dark-tree-current-hover'
      : 'hover:bg-light-tree-hover dark:hover:bg-dark-tree-hover'
    : 'hover:bg-light-hover dark:hover:bg-dark-hover'

  const os = detectPlatformOs()
  function dropModifiers(event: DragEvent) {
    if (os === 'macos') {
      return { ctrlKey: event.altKey, shiftKey: event.metaKey }
    }
    return { ctrlKey: event.ctrlKey, shiftKey: event.shiftKey }
  }

  function handleDragOver(event: DragEvent<HTMLLIElement>) {
    const drag = useDragStore.getState().drag
    if (!canDropInto(drag, node.path, os)) {
      return
    }
    event.preventDefault()
    event.dataTransfer.dropEffect = resolveDropKind(dropModifiers(event), drag!.sourceDir, node.path, os)
    setIsDropTarget(true)
  }

  function handleDragLeave(event: DragEvent<HTMLLIElement>) {
    if (event.currentTarget.contains(event.relatedTarget as Node | null)) {
      return
    }
    setIsDropTarget(false)
  }

  async function handleDrop(event: DragEvent<HTMLLIElement>) {
    event.preventDefault()
    const drag = useDragStore.getState().drag
    const modifiers = dropModifiers(event)
    setIsDropTarget(false)
    endDrag()
    await performDrop(drag, node.path, modifiers, os)
  }

  return (
    <li
      ref={rowRef}
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={(event) => void handleDrop(event)}
      onContextMenu={(event) => {
        event.preventDefault()
        openMenu({
          paneId: activePaneId,
          title: node.name,
          chip: 'DIR',
          x: event.clientX,
          y: event.clientY,
          ...buildContextMenuContent(
            activePaneId,
            { kind: 'tree', path: node.path },
            detectPlatformOs(),
          ),
        })
      }}
      className={`flex h-tree-row items-center gap-1 rounded-tab pr-2 text-row ${rowHoverClassName} ${rowBackgroundClassName} ${
        isDropTarget ? 'ring-2 ring-inset ring-accent-blue-border' : ''
      }`}
      style={rowStyle}
    >
      <button
        type="button"
        aria-label={`${node.expanded ? 'Collapse' : 'Expand'} ${node.name}`}
        onClick={() => void toggleTreeNode(node.path)}
        className="inline-flex h-6 w-6 cursor-pointer items-center justify-center rounded-tab focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border hover:bg-light-hover dark:hover:bg-dark-hover"
      >
        <ChevronRightIcon className={`h-3.5 w-3.5 ${node.expanded ? 'rotate-90' : ''}`} />
      </button>
      <button
        type="button"
        onClick={() => void navigatePane(activePaneId, node.path)}
        onMouseDown={(event) => {
          // Suppress the browser's middle-click autoscroll, which would
          // otherwise swallow the subsequent auxclick and prevent the
          // open-in-new-tab gesture from firing.
          if (event.button === 1) {
            event.preventDefault()
          }
        }}
        onAuxClick={(event) => {
          if (event.button === 1) {
            event.preventDefault()
            void openTabFromPath(activePaneId, node.path)
          }
        }}
        className="flex min-w-0 flex-1 cursor-pointer items-center gap-2 rounded-tab py-1 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border"
      >
        {volume ? (
          <VolumeIcon volume={volume} />
        ) : (
          <EntryIcon entry={{ name: node.name, isDir: true }} isOpen={node.expanded} />
        )}
        <span className="truncate">{node.name}</span>
      </button>
    </li>
  )
}
