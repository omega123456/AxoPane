import { memo, useState, type DragEvent, type MouseEvent, type ReactNode } from 'react'
import { ChevronRightIcon } from '@/components/icons'
import { EntryIcon } from '@/components/icons/EntryIcon'
import { VolumeIcon } from '@/components/icons/VolumeIcon'
import { SelectionPill } from '@/components/controls/SelectionPill'
import { CapacityBar } from './CapacityBar'
import { volumeUsedFraction } from '@/lib/volumes'
import { treeIndentPx } from '@/lib/tree-flat'
import { detectPlatformOs } from '@/lib/keymap'
import {
  canDropInto,
  dropModifiers,
  performDrop,
  resolveDropKind,
  setDropEffect,
} from '@/lib/drag-drop'
import type { VolumeInfo } from '@/lib/types/ipc'
import type { TreeNodeState } from '@/stores/panes-store'
import { useDragStore } from '@/stores/drag-store'
import { FOCUS_RING_INSET } from '@/lib/focus-ring'
import {
  disclosureChevronClass,
  dropTargetRing,
  HOVER_FILL,
  HOVER_ON_POINTER,
} from '@/lib/class-names'
import { middleClickOpenProps } from './middle-click-open'

export type TreeRowActions = {
  onToggle: (path: string) => void
  onNavigate: (path: string) => void
  onOpenTab: (path: string) => void
  onContextMenu: (node: TreeNodeState, event: MouseEvent) => void
}

type TreeNodeProps = {
  node: TreeNodeState
  depth: number
  volume?: VolumeInfo
  isCurrent: boolean
  isPinned?: boolean
  actions: TreeRowActions
}

const INLINE_CURRENT_FILL = `${HOVER_ON_POINTER} ${HOVER_FILL}`
const PINNED_CURRENT_FILL =
  'hover:bg-light-tree-current-hover dark:hover:bg-dark-tree-current-hover bg-light-tree-current dark:bg-dark-tree-current'
const PINNED_FILL =
  'hover:bg-light-tree-hover dark:hover:bg-dark-tree-hover bg-light-base dark:bg-dark-base'

function treeRowFillClass(isCurrent: boolean, isPinned: boolean) {
  if (isPinned) {
    return isCurrent ? PINNED_CURRENT_FILL : PINNED_FILL
  }
  return isCurrent ? INLINE_CURRENT_FILL : HOVER_ON_POINTER
}

function treeRowHeightClass(usedFraction: number | null) {
  return usedFraction === null ? 'h-tree-row' : 'h-tree-drive-row'
}

function TreeNodeLabel({
  name,
  volume,
  usedFraction,
}: {
  name: string
  volume: TreeNodeProps['volume']
  usedFraction: number | null
}) {
  return (
    <>
      {volume ? <VolumeIcon volume={volume} /> : <EntryIcon entry={{ name, isDir: true }} />}
      {usedFraction === null ? (
        <span className="truncate">{name}</span>
      ) : (
        <span className="flex min-w-0 flex-1 flex-col gap-1.25">
          <span className="truncate">{name}</span>
          <CapacityBar usedFraction={usedFraction} />
        </span>
      )}
    </>
  )
}

function useTreeDropTarget(path: string) {
  const [isDropTarget, setIsDropTarget] = useState(false)
  const platform = detectPlatformOs()

  function handleDragOver(event: DragEvent<HTMLDivElement>) {
    const drag = useDragStore.getState().drag
    if (drag?.kind !== 'file-transfer') {
      return
    }
    if (!canDropInto(drag, path, platform)) {
      return
    }
    event.preventDefault()
    setDropEffect(
      event.dataTransfer,
      resolveDropKind(
        dropModifiers(event, platform),
        { sourceDir: drag.sourceDir, destinationDir: path },
        platform,
      ),
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
    const modifiers = dropModifiers(event, platform)
    setIsDropTarget(false)
    useDragStore.getState().end()
    await performDrop(drag, { destinationDir: path, modifiers }, platform)
  }

  return { isDropTarget, handleDragOver, handleDragLeave, handleDrop }
}

type TreeNodeButtonProps = {
  node: TreeNodeState
  actions: TreeRowActions
}

function TreeToggleButton({ node, actions }: TreeNodeButtonProps) {
  return (
    <button
      type="button"
      aria-label={`Toggle ${node.name}`}
      onClick={() => actions.onToggle(node.path)}
      className={`-mx-1 inline-flex size-5 shrink-0 cursor-pointer items-center justify-center rounded-control text-light-text-tertiary dark:text-dark-text-tertiary ${HOVER_ON_POINTER} ${FOCUS_RING_INSET}`}
    >
      <ChevronRightIcon className={`size-3 ${disclosureChevronClass(node.expanded)}`} />
    </button>
  )
}

function TreeNavigateButton({
  node,
  actions,
  children,
}: TreeNodeButtonProps & { children: ReactNode }) {
  return (
    <button
      type="button"
      aria-label={node.name}
      onClick={() => actions.onNavigate(node.path)}
      {...middleClickOpenProps(() => actions.onOpenTab(node.path))}
      className={`flex h-full min-w-0 flex-1 cursor-pointer items-center gap-2 rounded-control text-left ${FOCUS_RING_INSET}`}
    >
      {children}
    </button>
  )
}

function TreeNodeImpl({
  node,
  depth,
  volume,
  isCurrent,
  isPinned = false,
  actions,
}: TreeNodeProps) {
  const { isDropTarget, handleDragOver, handleDragLeave, handleDrop } = useTreeDropTarget(node.path)
  const usedFraction = volume ? volumeUsedFraction(volume) : null

  return (
    <div
      role="treeitem"
      aria-expanded={node.expanded}
      data-tree-row={node.path}
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={(event) => void handleDrop(event)}
      onContextMenu={(event) => actions.onContextMenu(node, event)}
      className={`relative flex ${treeRowHeightClass(usedFraction)} items-center gap-2 rounded-control pr-2 text-row text-light-text dark:text-dark-text ${treeRowFillClass(isCurrent, isPinned)} ${dropTargetRing(isDropTarget)}`}
      style={{ paddingLeft: `${treeIndentPx(depth)}px` }}
    >
      {isCurrent ? <SelectionPill /> : null}
      <TreeToggleButton node={node} actions={actions} />
      <TreeNavigateButton node={node} actions={actions}>
        <TreeNodeLabel name={node.name} volume={volume} usedFraction={usedFraction} />
      </TreeNavigateButton>
    </div>
  )
}

export const TreeNode = memo(TreeNodeImpl)
