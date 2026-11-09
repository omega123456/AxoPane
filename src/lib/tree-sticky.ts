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
  if (!activePath) {
    return []
  }
  if (!treeNodes[activePath]) {
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

function isExpandedTowards(node: TreeNodeState | undefined, childPath: string) {
  return node?.expanded === true && node.children.includes(childPath)
}

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
  if (fold.showHiddenLevels) {
    return []
  }
  if (below.length <= FOLD_MAX_LEVELS) {
    return []
  }
  const hidden = below.slice(FOLD_KEEP_HEAD, below.length - (FOLD_MAX_LEVELS - FOLD_KEEP_HEAD))
  const chainIsOpen = hidden.every((path) => {
    const node = treeNodes[path]
    const next = fold.chain[fold.chain.indexOf(path) + 1]
    return isExpandedTowards(node, next)
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
export type VisibleTreeOptions = {
  excludedRootPaths?: ReadonlySet<string>
  fold?: TreeFoldState
}

export function flattenVisibleTree(
  treeNodes: Record<string, TreeNodeState>,
  rootPath: string,
  options: VisibleTreeOptions = {},
): FlatTreeRow[] {
  const { excludedRootPaths = new Set<string>(), fold = EMPTY_TREE_FOLD } = options
  const walk = new VisibleTreeWalk(treeNodes, excludedRootPaths, fold)
  walk.walk(rootPath, 0)
  return walk.rows
}

type TreePosition = {
  path: string
  depth: number
}

class VisibleTreeWalk {
  readonly rows: FlatTreeRow[] = []
  private readonly treeNodes: Record<string, TreeNodeState>
  private readonly excludedRootPaths: ReadonlySet<string>
  private readonly fold: TreeFoldState
  private readonly chainIndex: Map<string, number>
  private readonly hidden: string[]

  constructor(
    treeNodes: Record<string, TreeNodeState>,
    excludedRootPaths: ReadonlySet<string>,
    fold: TreeFoldState,
  ) {
    this.treeNodes = treeNodes
    this.excludedRootPaths = excludedRootPaths
    this.fold = fold
    this.chainIndex = new Map(fold.chain.map((path, index) => [path, index]))
    this.hidden = hiddenChainLevels(treeNodes, fold)
  }

  walk(path: string, depth: number): void {
    const node = this.treeNodes[path]
    if (this.isExcludedRoot(path, depth)) {
      return
    }
    if (!node) {
      return
    }
    this.rows.push({ kind: 'node', path, depth })
    if (!node.expanded) {
      return
    }

    const next = this.nextOnChain(path)
    if (this.fold.revealedSiblings.has(path)) {
      this.walkChildren(node, next, depth)
      return
    }
    if (!this.isFoldableChild(node, next)) {
      this.walkChildren(node, next, depth)
      return
    }
    this.walkFolded({ path, depth }, node, next)
  }

  private walkPath(path: string, depth: number): void {
    const { hidden } = this
    if (hidden[0] !== path) {
      this.walk(path, depth)
      return
    }
    this.rows.push({ kind: 'fold-levels', hiddenPaths: hidden, depth })
    const resume = this.fold.chain[(this.chainIndex.get(hidden[hidden.length - 1]) ?? 0) + 1]
    this.walk(resume, depth + hidden.length)
  }

  private nextOnChain(path: string) {
    const index = this.chainIndex.get(path)
    return index === undefined ? undefined : this.fold.chain[index + 1]
  }

  private isExcludedRoot(path: string, depth: number) {
    return depth > 0 && this.excludedRootPaths.has(path.toLowerCase())
  }

  private isFoldableChild(node: TreeNodeState, next: string | undefined): next is string {
    return (
      next !== undefined &&
      node.children.includes(next) &&
      !this.excludedRootPaths.has(next.toLowerCase())
    )
  }

  private walkChildren(node: TreeNodeState, next: string | undefined, depth: number) {
    for (const childPath of node.children) {
      this.visitChild(childPath, childPath === next, depth + 1)
    }
  }

  private visitChild(path: string, onActivePath: boolean, depth: number) {
    if (onActivePath) {
      this.walkPath(path, depth)
      return
    }
    this.walk(path, depth)
  }

  private walkFolded({ path, depth }: TreePosition, node: TreeNodeState, next: string) {
    this.walkPath(next, depth + 1)
    const others = node.children.length - 1
    if (others > 0) {
      this.rows.push({ kind: 'fold-siblings', parentPath: path, count: others, depth: depth + 1 })
    }
  }
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
