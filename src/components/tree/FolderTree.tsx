import { useEffect, useMemo } from 'react'
import { TreeNode } from './TreeNode'
import { TrashTreeRow } from './TrashTreeRow'
import { useLayoutStore } from '@/stores/layout-store'
import { usePanesStore } from '@/stores/panes-store'
import { groupVolumesByCategory } from '@/lib/volumes'
import { buildStickyChain, flattenVisibleTree } from '@/lib/tree-sticky'

export function FolderTree() {
  const volumes = usePanesStore((state) => state.volumes)
  const revealPath = usePanesStore((state) => state.revealPath)
  const activePaneId = usePanesStore((state) => state.activePaneId)
  const activePath = usePanesStore((state) => state.panes[activePaneId].path)
  const treeNodes = usePanesStore((state) => state.treeNodes)
  const treeWidthPx = useLayoutStore((state) => state.treeWidthPx)

  const groups = useMemo(() => groupVolumesByCategory(volumes), [volumes])
  // The ancestor chain (volume root -> active node) pins itself to the top of
  // the scroll area as the user scrolls past each ancestor's row, and
  // unpins again once scrolled back into its natural position - mirroring
  // editor "sticky scroll" breadcrumbs for the active folder's lineage.
  const stickyChain = useMemo(
    () => buildStickyChain(treeNodes, activePath),
    [treeNodes, activePath],
  )

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
      <div className="min-h-0 flex-1 overflow-auto pb-2 scrollbar-thin scrollbar-track-transparent scrollbar-thumb-light-text-faint dark:scrollbar-thumb-dark-text-faint">
        {/* A real (scrollable) spacer, not container padding: a sticky row's
            `top: 0` is measured from the scroll container's padding edge, so
            top padding here would leave a band where already-scrolled rows
            could still render above the pinned stack before being clipped. */}
        <div className="h-2" aria-hidden="true" />
        {groups.map((group) => (
          <section key={group.category} className="mb-2">
            <p className="px-3 pb-1 pt-1 font-mono text-uxs uppercase tracking-wide text-light-text-faint dark:text-dark-text-faint">
              {group.label}
            </p>
            <div className="space-y-1">
              {group.volumes.map((volume) => (
                // One flat <ul> per volume: every visible row is a direct <li>
                // child, so each sticky row's containing block is this tall list
                // (spanning the whole subtree) rather than its own short <li>.
                // That is what lets the entire ancestor chain pin instead of
                // just the top few - see flattenVisibleTree.
                <ul key={volume.mountRoot}>
                  {flattenVisibleTree(treeNodes, volume.mountRoot).map((row) => (
                    <TreeNode
                      key={row.path}
                      path={row.path}
                      depth={row.depth}
                      volume={row.depth === 0 ? volume : undefined}
                      stickyChain={stickyChain}
                    />
                  ))}
                </ul>
              ))}
              {group.category === 'fixed' && (
                <ul>
                  <TrashTreeRow />
                </ul>
              )}
            </div>
          </section>
        ))}
      </div>
    </aside>
  )
}
