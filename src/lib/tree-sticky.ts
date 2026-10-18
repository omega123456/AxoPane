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
/** Pinned rows beyond the volume root: the active folder plus its two nearest ancestors. */
export const STICKY_MAX_ANCESTORS = 3

/** Path levels below the volume root shown before the middle folds into one row. */
export const FOLD_MAX_LEVELS = 6
const FOLD_KEEP_HEAD = 2

/**
 * Which parts of the active path the tree folds. `chain` is the active path's
 * ancestor chain (from `buildStickyChain`); `revealedSiblings` holds the chain
 * folders whose other children the user opened.
 */
export type TreeFoldState = {
  chain: string[]
  revealedSiblings: ReadonlySet<string>
  showHiddenLevels: boolean
}

export const EMPTY_TREE_FOLD: TreeFoldState = {
  chain: [],
  revealedSiblings: new Set(),
  showHiddenLevels: false,
}

/** A single visible tree row, flattened out of the nested node graph. */
export type FlatTreeRow =
  | {
      kind: 'node'
      path: string
      /** Nesting level from the volume root (0 = the root itself); drives indentation. */
      depth: number
    }
  | { kind: 'fold-siblings'; parentPath: string; count: number; depth: number }
  | { kind: 'fold-levels'; hiddenPaths: string[]; depth: number }

/**
 * Middle levels of a long active path that fold into one "N levels hidden"
 * row. Only folds when every hidden level is expanded down the chain, so the
 * folded row always stands in for rows that would otherwise render.
 */
function hiddenChainLevels(
  treeNodes: Record<string, TreeNodeState>,
  fold: TreeFoldState,
): string[] {
  const below = fold.chain.slice(1)
  if (fold.showHiddenLevels || below.length <= FOLD_MAX_LEVELS) {
    return []
  }
  const hidden = below.slice(FOLD_KEEP_HEAD, below.length - (FOLD_MAX_LEVELS - FOLD_KEEP_HEAD))
  const chainIsOpen = hidden.every((path) => {
    const node = treeNodes[path]
    const next = fold.chain[fold.chain.indexOf(path) + 1]
    return node?.expanded === true && node.children.includes(next)
  })
  return chainIsOpen ? hidden : []
}

/**
 * Depth-first flatten of the visible subtree under `rootPath` (the root plus
 * every descendant reachable through `expanded` nodes), in render order.
 *
 * Folders on the active path (`fold.chain`) are folded: each shows only its
 * next path folder followed by one "N other folders" row, unless the user
 * revealed that level. A path deeper than `FOLD_MAX_LEVELS` also folds its
 * middle levels into one row. Folders off the active path walk unchanged.
 */
export function flattenVisibleTree(
  treeNodes: Record<string, TreeNodeState>,
  rootPath: string,
  excludedRootPaths: ReadonlySet<string> = new Set(),
  fold: TreeFoldState = EMPTY_TREE_FOLD,
): FlatTreeRow[] {
  const rows: FlatTreeRow[] = []
  const chainIndex = new Map(fold.chain.map((path, index) => [path, index]))
  const hidden = hiddenChainLevels(treeNodes, fold)

  const walkPath = (path: string, depth: number): void => {
    if (hidden[0] !== path) {
      walk(path, depth)
      return
    }
    rows.push({ kind: 'fold-levels', hiddenPaths: hidden, depth })
    const resume = fold.chain[(chainIndex.get(hidden[hidden.length - 1]) ?? 0) + 1]
    walk(resume, depth + hidden.length)
  }

  const walk = (path: string, depth: number): void => {
    if (depth > 0 && excludedRootPaths.has(path.toLowerCase())) {
      return
    }

    const node = treeNodes[path]
    if (!node) {
      return
    }
    rows.push({ kind: 'node', path, depth })
    if (!node.expanded) {
      return
    }

    const index = chainIndex.get(path)
    const next = index === undefined ? undefined : fold.chain[index + 1]
    const foldable =
      next !== undefined &&
      node.children.includes(next) &&
      !excludedRootPaths.has(next.toLowerCase())
    if (!foldable || fold.revealedSiblings.has(path)) {
      for (const childPath of node.children) {
        if (childPath === next) {
          walkPath(childPath, depth + 1)
        } else {
          walk(childPath, depth + 1)
        }
      }
      return
    }

    walkPath(next, depth + 1)
    const others = node.children.length - 1
    if (others > 0) {
      rows.push({ kind: 'fold-siblings', parentPath: path, count: others, depth: depth + 1 })
    }
  }
  walk(rootPath, 0)
  return rows
}

/**
 * Caps the pinned ancestor chain to the volume root plus the nearest
 * `STICKY_MAX_ANCESTORS` rendered path folders. Entries folded out of the tree
 * (absent from `indexByPath`) are skipped.
 */
export function capStickyChain(
  chain: string[],
  indexByPath: ReadonlyMap<string, number>,
): string[] {
  const rendered = chain.filter((path) => indexByPath.has(path))
  if (rendered.length <= STICKY_MAX_ANCESTORS + 1) {
    return rendered
  }
  return [rendered[0], ...rendered.slice(-STICKY_MAX_ANCESTORS)]
}
