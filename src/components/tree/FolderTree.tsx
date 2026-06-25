import { useEffect } from 'react'
import { TreeNode } from './TreeNode'
import { isPathInsideVolume } from '@/lib/volumes'
import { useLayoutStore } from '@/stores/layout-store'
import { usePanesStore } from '@/stores/panes-store'

export function FolderTree() {
  const treeRoots = usePanesStore((state) => state.treeRoots)
  const ensureTreeChildren = usePanesStore((state) => state.ensureTreeChildren)
  const activePaneId = usePanesStore((state) => state.activePaneId)
  const activePath = usePanesStore((state) => state.panes[activePaneId].path)
  const treeWidth = useLayoutStore((state) => state.treeWidth)

  useEffect(() => {
    const root = treeRoots.find((candidate) => isPathInsideVolume(activePath, candidate))

    if (root) {
      void ensureTreeChildren(root)
    }
  }, [activePath, ensureTreeChildren, treeRoots])

  return (
    <aside
      className={`flex shrink-0 flex-col border-r border-light-border bg-light-tree dark:border-dark-border dark:bg-dark-tree ${
        treeWidth === 'compact' ? 'w-44' : treeWidth === 'wide' ? 'w-64' : 'w-tree'
      }`}
    >
      <div className="border-b border-light-border px-3 py-3 dark:border-dark-border">
        <p className="font-mono text-uxs uppercase tracking-wide text-light-text-muted dark:text-dark-text-muted">
          Shared tree
        </p>
      </div>
      <div className="min-h-0 flex-1 overflow-auto py-2">
        <ul className="space-y-1">
          {treeRoots.map((path) => (
            <TreeNode key={path} path={path} depth={0} />
          ))}
        </ul>
      </div>
    </aside>
  )
}
