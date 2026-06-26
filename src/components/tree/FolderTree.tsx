import { useEffect, useMemo } from 'react'
import { TreeNode } from './TreeNode'
import { useLayoutStore } from '@/stores/layout-store'
import { usePanesStore } from '@/stores/panes-store'
import { groupVolumesByCategory } from '@/lib/volumes'

export function FolderTree() {
  const volumes = usePanesStore((state) => state.volumes)
  const revealPath = usePanesStore((state) => state.revealPath)
  const activePaneId = usePanesStore((state) => state.activePaneId)
  const activePath = usePanesStore((state) => state.panes[activePaneId].path)
  const treeWidth = useLayoutStore((state) => state.treeWidth)

  const groups = useMemo(() => groupVolumesByCategory(volumes), [volumes])

  // Track the active folder: expand the ancestor chain down to it so the node is
  // rendered (TreeNode then scrolls it into view).
  useEffect(() => {
    void revealPath(activePath)
  }, [activePath, revealPath])

  return (
    <aside
      className={`flex min-h-0 shrink-0 flex-col border-r border-light-border bg-light-tree dark:border-dark-border dark:bg-dark-tree ${
        treeWidth === 'compact' ? 'w-44' : treeWidth === 'wide' ? 'w-64' : 'w-tree'
      }`}
    >
      <div className="border-b border-light-border px-3 py-3 dark:border-dark-border">
        <p className="font-mono text-uxs uppercase tracking-wide text-light-text-muted dark:text-dark-text-muted">
          Shared tree
        </p>
      </div>
      <div className="min-h-0 flex-1 overflow-auto py-2">
        {groups.map((group) => (
          <section key={group.category} className="mb-2">
            <p className="px-3 pb-1 pt-1 font-mono text-uxs uppercase tracking-wide text-light-text-faint dark:text-dark-text-faint">
              {group.label}
            </p>
            <ul className="space-y-1">
              {group.volumes.map((volume) => (
                <TreeNode key={volume.mountRoot} path={volume.mountRoot} depth={0} />
              ))}
            </ul>
          </section>
        ))}
      </div>
    </aside>
  )
}
