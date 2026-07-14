import { memo, useState, type DragEvent, type MouseEvent } from 'react'
import { ChevronRightIcon } from '@/components/icons'
import { EntryIcon } from '@/components/icons/EntryIcon'
import { VolumeIcon } from '@/components/icons/VolumeIcon'
import { detectPlatformOs } from '@/lib/keymap'
import { canDropInto, performDrop, resolveDropKind } from '@/lib/drag-drop'
import type { VolumeInfo } from '@/lib/types/ipc'
import type { TreeNodeState } from '@/stores/panes-store'
import { useDragStore } from '@/stores/drag-store'

/**
 * Stable, path-keyed handler dispatcher for the tree's rows.
 *
 * `FolderTree` builds exactly one of these (memoized so its identity never
 * changes across renders) and every `TreeNode` passes its own path when calling
 * in. Keeping this a single stable prop — instead of fresh per-row closures — is
 * what lets `React.memo` below skip re-rendering rows whose own props are
 * unchanged, and it moves every panes-store subscription out of the (formerly
 * ~8×-per-row) rows and up into the single `FolderTree` parent.
 */
export type TreeRowActions = {
  onToggle: (path: string) => void
  onNavigate: (path: string) => void
  onOpenTab: (path: string) => void
  onContextMenu: (node: TreeNodeState, event: MouseEvent) => void
}

type TreeNodeProps = {
  node: TreeNodeState
  depth: number
  /** Set for volume roots; switches the row glyph from a folder to a drive icon. */
  volume?: VolumeInfo
  /** True when this node is the active pane's current folder. */
  isCurrent: boolean
  /**
   * True when this is a copy rendered in the pinned ancestor overlay. Pinned
   * rows use opaque background tokens so rows scrolling underneath them don't
   * bleed through the (otherwise translucent) selection tint.
   */
  isPinned?: boolean
  actions: TreeRowActions
}

function TreeNodeImpl({
  node,
  depth,
  volume,
  isCurrent,
  isPinned = false,
  actions,
}: TreeNodeProps) {
  const [isDropTarget, setIsDropTarget] = useState(false)

  const rowStyle = {
    // Styling-constraint exception: runtime geometry only. Indentation is a
    // function of the (unbounded) tree depth, so it cannot be a static
    // utility/token. Colors/spacing elsewhere remain pure Tailwind utilities.
    paddingLeft: `${depth * 12 + 8}px`,
  }

  // accent-blue-soft is translucent; on a pinned row that translucency would
  // let scrolled-under rows show through, so pinned+current rows get the
  // opaque *-tree-current stand-in instead.
  const rowBackgroundClassName = isCurrent
    ? isPinned
      ? 'bg-light-tree-current text-accent-blue-light dark:bg-dark-tree-current dark:text-accent-blue'
      : 'bg-accent-blue-soft text-accent-blue-light dark:text-accent-blue'
    : isPinned
      ? 'bg-light-tree text-light-text-soft dark:bg-dark-tree dark:text-dark-text-soft'
      : 'text-light-text-soft dark:text-dark-text-soft'
  const rowHoverClassName = isPinned
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

  function handleDragOver(event: DragEvent<HTMLDivElement>) {
    const drag = useDragStore.getState().drag
    if (drag?.kind !== 'file-transfer' || !canDropInto(drag, node.path, os)) {
      return
    }
    event.preventDefault()
    event.dataTransfer.dropEffect = resolveDropKind(
      dropModifiers(event),
      drag.sourceDir,
      node.path,
      os,
    )
    setIsDropTarget(true)
  }

  function handleDragLeave(event: DragEvent<HTMLDivElement>) {
    if (event.currentTarget.contains(event.relatedTarget as Node | null)) {
      return
    }
    setIsDropTarget(false)
  }

  async function handleDrop(event: DragEvent<HTMLDivElement>) {
    event.preventDefault()
    const drag = useDragStore.getState().drag
    const modifiers = dropModifiers(event)
    setIsDropTarget(false)
    useDragStore.getState().end()
    await performDrop(drag, node.path, modifiers, os)
  }

  return (
    <div
      role="treeitem"
      data-tree-row={node.path}
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={(event) => void handleDrop(event)}
      onContextMenu={(event) => actions.onContextMenu(node, event)}
      className={`flex h-tree-row items-center gap-1 rounded-tab pr-2 text-row ${rowHoverClassName} ${rowBackgroundClassName} ${
        isDropTarget ? 'ring-2 ring-inset ring-accent-blue-border' : ''
      }`}
      style={rowStyle}
    >
      <button
        type="button"
        aria-label={`${node.expanded ? 'Collapse' : 'Expand'} ${node.name}`}
        onClick={() => actions.onToggle(node.path)}
        className="inline-flex h-6 w-6 cursor-pointer items-center justify-center rounded-tab focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border hover:bg-light-hover dark:hover:bg-dark-hover"
      >
        <ChevronRightIcon className={`h-3.5 w-3.5 ${node.expanded ? 'rotate-90' : ''}`} />
      </button>
      <button
        type="button"
        onClick={() => actions.onNavigate(node.path)}
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
            actions.onOpenTab(node.path)
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
    </div>
  )
}

/**
 * Memoized so scrolling / unrelated store updates only re-render the handful of
 * rows whose own props actually change. This works because `actions` is a
 * single stable object (see `TreeRowActions`) rather than fresh per-row
 * closures — mirroring `FileRow` in the main pane.
 */
export const TreeNode = memo(TreeNodeImpl)
