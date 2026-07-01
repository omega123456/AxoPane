import { useEffect, useMemo } from 'react'
import { TreeNode } from './TreeNode'
import { TrashTreeRow } from './TrashTreeRow'
import { useLayoutStore } from '@/stores/layout-store'
import { usePanesStore } from '@/stores/panes-store'
import { groupVolumesByCategory } from '@/lib/volumes'

export function FolderTree() {
  const volumes = usePanesStore((state) => state.volumes)
  const revealPath = usePanesStore((state) => state.revealPath)
  const activePaneId = usePanesStore((state) => state.activePaneId)
  const activePath = usePanesStore((state) => state.panes[activePaneId].path)
  const treeWidthPx = useLayoutStore((state) => state.treeWidthPx)

  const groups = useMemo(() => groupVolumesByCategory(volumes), [volumes])

  // Track the active folder: expand the ancestor chain down to it so the node is
  // rendered (TreeNode then scrolls it into view).
  useEffect(() => {
    void revealPath(activePath)
  }, [activePath, revealPath])

  return (
    <aside
      // Width is user-draggable runtime geometry; no static token can express it.
      style={{ width: `${treeWidthPx}px` }}
      className="flex min-h-0 shrink-0 flex-col border-r border-light-border bg-light-tree dark:border-dark-border dark:bg-dark-tree"
    >
      <div className="min-h-0 flex-1 overflow-auto py-2 scrollbar-thin scrollbar-track-transparent scrollbar-thumb-light-text-faint dark:scrollbar-thumb-dark-text-faint">
        {groups.map((group) => (
          <section key={group.category} className="mb-2">
            <p className="px-3 pb-1 pt-1 font-mono text-uxs uppercase tracking-wide text-light-text-faint dark:text-dark-text-faint">
              {group.label}
            </p>
            <ul className="space-y-1">
              {group.volumes.map((volume) => (
                <TreeNode key={volume.mountRoot} path={volume.mountRoot} depth={0} volume={volume} />
              ))}
              {group.category === 'fixed' && <TrashTreeRow />}
            </ul>
          </section>
        ))}
      </div>
    </aside>
  )
}
