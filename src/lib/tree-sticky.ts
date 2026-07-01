import type { TreeNodeState } from '@/stores/panes-store'

/**
 * Ordered ancestor chain (volume root -> ... -> active node) for the
 * currently active tree path. `revealPath` keeps exactly these nodes
 * expanded, so this chain is what FolderTree pins to the top of the
 * scroll area as the user scrolls through a deeply nested folder.
 */
export function buildStickyChain(
  treeNodes: Record<string, TreeNodeState>,
  activePath: string | null,
): string[] {
  if (!activePath || !treeNodes[activePath]) {
    return []
  }

  const chain: string[] = []
  let current: string | null = activePath
  while (current) {
    chain.unshift(current)
    current = treeNodes[current]?.parentPath ?? null
  }
  return chain
}

/** A single visible tree row, flattened out of the nested node graph. */
export type FlatTreeRow = {
  path: string
  /** Nesting level from the volume root (0 = the root itself); drives indentation. */
  depth: number
}

/**
 * Depth-first flatten of the visible subtree under `rootPath` (the root plus
 * every descendant reachable through `expanded` nodes), in render order.
 *
 * FolderTree renders these as direct `<li>` siblings of one `<ul>` per volume
 * rather than as nested lists. That flat structure is what makes sticky rows
 * work: a `position: sticky` row is confined to its containing block, and with
 * nested `<ul>`s that block is the row's own (often single-row-tall) `<li>`, so
 * deep ancestors can never travel far enough to pin. As flat siblings, every
 * row's containing block is the tall per-volume `<ul>`, so the whole ancestor
 * chain pins - and all rows share one stacking context, so pinned rows
 * (positive z-index) reliably paint above scrolling in-flow rows.
 */
export function flattenVisibleTree(
  treeNodes: Record<string, TreeNodeState>,
  rootPath: string,
): FlatTreeRow[] {
  const rows: FlatTreeRow[] = []
  const walk = (path: string, depth: number): void => {
    const node = treeNodes[path]
    if (!node) {
      return
    }
    rows.push({ path, depth })
    if (node.expanded) {
      for (const childPath of node.children) {
        walk(childPath, depth + 1)
      }
    }
  }
  walk(rootPath, 0)
  return rows
}
