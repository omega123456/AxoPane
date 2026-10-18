import { memo } from 'react'
import { ChevronsUpDownIcon, EllipsisIcon } from '@/components/icons'
import { FOCUS_RING_INSET } from '@/lib/focus-ring'
import { treeIndentPx } from '@/lib/tree-flat'

type TreeFoldRowProps =
  | { kind: 'siblings'; count: number; depth: number; onReveal: () => void }
  | { kind: 'levels'; hiddenNames: string[]; depth: number; onReveal: () => void }

/**
 * Stand-in for tree rows folded away along the active path: either a level's
 * other folders ("12 other folders") or the middle of a very deep path
 * ("4 levels hidden"). Activating it shows the folded rows.
 */
function TreeFoldRowImpl(props: TreeFoldRowProps) {
  const label =
    props.kind === 'siblings'
      ? `${props.count} other ${props.count === 1 ? 'folder' : 'folders'}`
      : `${props.hiddenNames.length} levels hidden`
  const Icon = props.kind === 'siblings' ? EllipsisIcon : ChevronsUpDownIcon

  return (
    <div
      role="treeitem"
      aria-expanded={false}
      data-tree-fold={props.kind}
      className="flex h-tree-row items-center rounded-control pr-2"
      // Styling-constraint exception: runtime geometry only, matching TreeNode's
      // depth-based indentation.
      style={{ paddingLeft: `${treeIndentPx(props.depth)}px` }}
    >
      <button
        type="button"
        onClick={props.onReveal}
        title={props.kind === 'levels' ? props.hiddenNames.join(' › ') : undefined}
        // The icon sits in the chevron column, which keeps the label readable at
        // the indent cap.
        className={`flex h-full min-w-0 flex-1 cursor-pointer items-center gap-2 rounded-control text-left text-row text-light-text-tertiary hover:bg-light-hover hover:text-light-text dark:text-dark-text-tertiary dark:hover:bg-dark-hover dark:hover:text-dark-text ${FOCUS_RING_INSET}`}
      >
        <Icon aria-hidden="true" className="size-4 flex-none" />
        <span className="truncate">{label}</span>
      </button>
    </div>
  )
}

export const TreeFoldRow = memo(TreeFoldRowImpl)
