import { buildContextMenuItems } from '@/components/menus/menu-definitions'
import { ChevronRightIcon, FolderIcon } from '@/components/icons'
import { detectPlatformOs } from '@/lib/keymap'
import { useContextMenuStore } from '@/stores/context-menu-store'
import { usePanesStore } from '@/stores/panes-store'

type TreeNodeProps = {
  path: string
  depth: number
}

export function TreeNode({ path, depth }: TreeNodeProps) {
  const node = usePanesStore((state) => state.treeNodes[path])
  const activePaneId = usePanesStore((state) => state.activePaneId)
  const activePath = usePanesStore((state) => state.panes[activePaneId].path)
  const toggleTreeNode = usePanesStore((state) => state.toggleTreeNode)
  const navigatePane = usePanesStore((state) => state.navigatePane)
  const openMenu = useContextMenuStore((state) => state.openMenu)

  if (!node) {
    return null
  }

  const isCurrent = activePath === node.path

  return (
    <li>
      <div
        onContextMenu={(event) => {
          event.preventDefault()
          openMenu({
            paneId: activePaneId,
            title: node.name,
            chip: 'DIR',
            x: event.clientX,
            y: event.clientY,
            items: buildContextMenuItems(activePaneId, { kind: 'tree', path: node.path }, detectPlatformOs()),
          })
        }}
        className={`flex items-center gap-1 rounded-tab pr-2 text-row ${
          isCurrent ? 'bg-accent-blue-soft text-accent-blue-light dark:text-accent-blue' : 'text-light-text-soft dark:text-dark-text-soft'
        }`}
        // Styling-constraint exception: runtime geometry only. Indentation is a
        // function of the (unbounded) tree depth, so it cannot be a static
        // utility/token. Colors/spacing above remain pure Tailwind utilities.
        style={{ paddingLeft: `${depth * 12 + 8}px` }}
      >
        <button
          type="button"
          aria-label={`${node.expanded ? 'Collapse' : 'Expand'} ${node.name}`}
          onClick={() => void toggleTreeNode(node.path)}
          className="inline-flex h-6 w-6 items-center justify-center rounded-tab focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border hover:bg-light-hover dark:hover:bg-dark-hover"
        >
          <ChevronRightIcon className={`h-3.5 w-3.5 ${node.expanded ? 'rotate-90' : ''}`} />
        </button>
        <button
          type="button"
          onClick={() => void navigatePane(activePaneId, node.path)}
          className="flex min-w-0 flex-1 items-center gap-2 rounded-tab py-1 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border"
        >
          <FolderIcon className="h-4 w-4 shrink-0" />
          <span className="truncate">{node.name}</span>
        </button>
      </div>
      {node.expanded && node.children.length > 0 ? (
        <ul>
          {node.children.map((childPath) => (
            <TreeNode key={childPath} path={childPath} depth={depth + 1} />
          ))}
        </ul>
      ) : null}
    </li>
  )
}
