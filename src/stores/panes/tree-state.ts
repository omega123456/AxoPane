import type { DirectoryEntry, DirPatchEvent, TreeChildEntry, VolumeInfo } from '@/lib/types/ipc'
import { pathKey, pathsMatch } from '@/lib/path-compare'
import { formatVolumeTreeName, isPathInsideVolume, sortVolumesForTree } from '@/lib/volumes'
import { expandabilityOf, pruneTreeCache, type LazyTreeNode } from '@/stores/panes/tree-cache'
import { basenameFromPath, getParentPath } from '@/stores/panes/pane-paths'
import type { PanesData, TreeNodeState, TreeNodes } from '@/stores/panes/pane-state'

type TreeState = Pick<PanesData, 'treeNodes' | 'treeRoots'>

type ListedChildNode = { nodePath: string; node: TreeNodeState }

const UNKNOWN_EXPANDABILITY = 'unknown' as const

const NATURAL_NAME_ORDER: Intl.CollatorOptions = { numeric: true, sensitivity: 'base' }

export function treeRootForPath(treeRoots: string[], path: string) {
  return (
    treeRoots.find((root) => root === path) ??
    treeRoots.find((root) => pathsMatch(root, path)) ??
    null
  )
}

function retainedRootNodeState(existing: TreeNodeState | undefined) {
  if (!existing) {
    return {
      children: [],
      expanded: false,
      loaded: false,
      expandability: UNKNOWN_EXPANDABILITY,
      lastAccess: Date.now(),
    }
  }
  return {
    children: existing.children,
    expanded: existing.expanded,
    loaded: existing.loaded,
    expandability: existing.expandability ?? UNKNOWN_EXPANDABILITY,
    lastAccess: existing.lastAccess ?? Date.now(),
  }
}

function volumeRootNode(volume: VolumeInfo, existing: TreeNodeState | undefined): TreeNodeState {
  return {
    id: volume.mountRoot,
    name: formatVolumeTreeName(volume),
    path: volume.mountRoot,
    parentPath: null,
    ...retainedRootNodeState(existing),
  }
}

export function buildRootTreeState(volumes: VolumeInfo[], currentNodes: TreeNodes = {}): TreeState {
  const sortedVolumes = sortVolumesForTree(volumes)
  const treeRoots = sortedVolumes.map((volume) => volume.mountRoot)
  const isRetainedChild = (node: TreeNodeState) =>
    node.parentPath !== null && treeRoots.some((root) => isPathInsideVolume(node.path, root))
  const treeNodes = Object.fromEntries(
    Object.entries(currentNodes).filter(([, node]) => isRetainedChild(node)),
  ) as TreeNodes

  for (const volume of sortedVolumes) {
    treeNodes[volume.mountRoot] = volumeRootNode(volume, currentNodes[volume.mountRoot])
  }

  return { treeRoots, treeNodes }
}

export function prunedTreeNodes(
  treeNodes: TreeNodes,
  state: Pick<PanesData, 'panes'>,
  treeRoots: string[],
): TreeNodes {
  return pruneTreeCache(treeNodes as Record<string, LazyTreeNode>, [
    state.panes.left.path,
    state.panes.right.path,
    ...treeRoots,
  ]) as TreeNodes
}

export function withVolumes(
  state: Pick<PanesData, 'panes' | 'treeNodes'>,
  volumes: VolumeInfo[],
): Pick<PanesData, 'volumes' | 'treeRoots' | 'treeNodes'> {
  const nextTree = buildRootTreeState(volumes, state.treeNodes)
  return {
    volumes,
    treeRoots: nextTree.treeRoots,
    treeNodes: prunedTreeNodes(nextTree.treeNodes, state, nextTree.treeRoots),
  }
}

function loadedNodeIdentity(
  path: string,
  currentNode: TreeNodeState | undefined,
  treeRoots: string[],
) {
  const isRoot = Boolean(treeRootForPath(treeRoots, path))
  return {
    id: path,
    name: currentNode?.name ?? basenameFromPath(path),
    path,
    parentPath: isRoot ? null : (currentNode?.parentPath ?? getParentPath(path)),
  }
}

function expandedAfterLoad(currentNode: TreeNodeState | undefined, expandNewNode: boolean) {
  if (currentNode == null) {
    return expandNewNode
  }
  const unloadedRoot = currentNode.parentPath === null && !currentNode.loaded
  return currentNode.expanded || unloadedRoot
}

function listedChildNode(
  state: TreeState,
  parentPath: string,
  entry: TreeChildEntry,
): ListedChildNode {
  const volumeRoot = treeRootForPath(state.treeRoots, entry.path)
  const nodePath = volumeRoot ?? entry.path
  const existing = state.treeNodes[nodePath] ?? state.treeNodes[entry.path]
  const listedParentPath = volumeRoot ? null : parentPath
  if (existing) {
    const name = volumeRoot ? existing.name : entry.name
    return { nodePath, node: { ...existing, name, parentPath: listedParentPath } }
  }
  return {
    nodePath,
    node: {
      id: nodePath,
      name: entry.name,
      path: nodePath,
      parentPath: listedParentPath,
      children: [],
      expanded: false,
      loaded: false,
      expandability: expandabilityOf(entry),
      lastAccess: Date.now(),
    },
  }
}

function listedChildNodes(state: TreeState, parentPath: string, children: TreeChildEntry[]) {
  const nodes: TreeNodes = {}
  for (const entry of children) {
    const listed = listedChildNode(state, parentPath, entry)
    nodes[listed.nodePath] = listed.node
  }
  return nodes
}

export function loadedTreeNodes(
  state: TreeState,
  path: string,
  currentNode: TreeNodeState | undefined,
  children: TreeChildEntry[],
): TreeNodes {
  return {
    ...state.treeNodes,
    [path]: {
      ...loadedNodeIdentity(path, currentNode, state.treeRoots),
      children: children.map((entry) => treeRootForPath(state.treeRoots, entry.path) ?? entry.path),
      expanded: expandedAfterLoad(currentNode, true),
      loaded: true,
      expandability: children.length === 0 ? 'empty' : 'nonEmpty',
      lastAccess: Date.now(),
    },
    ...listedChildNodes(state, path, children),
  }
}

export function failedTreeNodes(
  state: TreeState,
  path: string,
  currentNode: TreeNodeState | undefined,
): TreeNodes {
  return {
    ...state.treeNodes,
    [path]: {
      ...loadedNodeIdentity(path, currentNode, state.treeRoots),
      children: [],
      expanded: expandedAfterLoad(currentNode, false),
      loaded: true,
      expandability: currentNode?.expandability ?? 'empty',
      lastAccess: Date.now(),
    },
  }
}

export function revealChain(path: string, root: string): string[] {
  const chain: string[] = []
  let current: string | null = path
  while (current) {
    chain.unshift(current)
    if (pathsMatch(current, root)) {
      break
    }
    current = getParentPath(current)
  }
  return chain
}

function expandedForReveal(
  nodePath: string,
  ancestors: Set<string>,
  leaf: string,
  expanded: boolean,
) {
  const nodeKey = pathKey(nodePath)
  if (ancestors.has(nodeKey)) {
    return true
  }
  if (nodeKey === leaf) {
    return expanded
  }
  return false
}

export function revealedTree(
  state: Pick<PanesData, 'treeNodes' | 'treeFold'>,
  path: string,
  chain: string[],
): Pick<PanesData, 'treeNodes' | 'treeFold'> {
  const ancestors = new Set(chain.slice(0, -1).map((entry) => pathKey(entry)))
  const leaf = pathKey(path)
  const treeFold =
    state.treeFold.leaf === leaf
      ? state.treeFold
      : { leaf, revealedSiblings: [], showHiddenLevels: false }
  const treeNodes = { ...state.treeNodes }
  for (const [nodePath, node] of Object.entries(state.treeNodes)) {
    const expanded = expandedForReveal(nodePath, ancestors, leaf, node.expanded)
    if (expanded !== node.expanded) {
      treeNodes[nodePath] = { ...node, expanded }
    }
  }
  return { treeNodes, treeFold }
}

function placeholderTreeNode(path: string): Omit<TreeNodeState, 'expanded'> {
  return {
    id: path,
    name: basenameFromPath(path),
    path,
    parentPath: getParentPath(path),
    children: [],
    loaded: false,
    expandability: UNKNOWN_EXPANDABILITY,
    lastAccess: Date.now(),
  }
}

export function toggledTreeNode(
  state: Pick<PanesData, 'treeNodes'>,
  path: string,
): Pick<PanesData, 'treeNodes'> {
  const node = state.treeNodes[path]
  return {
    treeNodes: {
      ...state.treeNodes,
      [path]: { ...(node ?? placeholderTreeNode(path)), expanded: !node?.expanded },
    },
  }
}

export function withRevealedSiblings(
  state: Pick<PanesData, 'treeFold'>,
  parentPath: string,
): Pick<PanesData, 'treeFold'> | null {
  if (state.treeFold.revealedSiblings.includes(parentPath)) {
    return null
  }
  return {
    treeFold: {
      ...state.treeFold,
      revealedSiblings: [...state.treeFold.revealedSiblings, parentPath],
    },
  }
}

export function withHiddenLevelsShown(
  state: Pick<PanesData, 'treeFold'>,
): Pick<PanesData, 'treeFold'> {
  return { treeFold: { ...state.treeFold, showHiddenLevels: true } }
}

function treeNodeKeyForPath(treeNodes: TreeNodes, path: string) {
  if (treeNodes[path]) {
    return path
  }
  return Object.keys(treeNodes).find((nodePath) => pathsMatch(nodePath, path)) ?? null
}

function treeNodeAtPath(treeNodes: TreeNodes, path: string) {
  const nodeKey = treeNodeKeyForPath(treeNodes, path)
  return nodeKey ? treeNodes[nodeKey] : undefined
}

function childPathInList(children: string[], path: string) {
  return children.find((childPath) => pathsMatch(childPath, path)) ?? null
}

function collectTreeSubtreePaths(treeNodes: TreeNodes, path: string, collected: Set<string>) {
  if (collected.has(path)) {
    return
  }
  collected.add(path)
  for (const childPath of treeNodes[path]?.children ?? []) {
    collectTreeSubtreePaths(treeNodes, childPath, collected)
  }
}

function compareTreeChildPaths(treeNodes: TreeNodes, leftPath: string, rightPath: string) {
  const leftName = treeNodes[leftPath]?.name ?? basenameFromPath(leftPath)
  const rightName = treeNodes[rightPath]?.name ?? basenameFromPath(rightPath)
  return leftName.localeCompare(rightName, undefined, NATURAL_NAME_ORDER)
}

function visibleDirectory(entry: DirectoryEntry | null, showHiddenFiles: boolean) {
  if (!entry?.isDir) {
    return null
  }
  const hiddenFromTree = !showHiddenFiles && (entry.isHidden || entry.isSystem)
  return hiddenFromTree ? null : entry
}

function inheritedExpandability(
  previous: TreeNodeState | undefined,
  existing: TreeNodeState | undefined,
) {
  return previous?.expandability ?? existing?.expandability ?? UNKNOWN_EXPANDABILITY
}

function inheritedTreeNodeState(
  previous: TreeNodeState | undefined,
  existing: TreeNodeState | undefined,
) {
  const source = previous ?? existing
  return {
    children: source?.children ?? [],
    expanded: source?.expanded ?? false,
    loaded: source?.loaded ?? false,
    expandability: inheritedExpandability(previous, existing),
  }
}

class TreePatchDraft {
  private treeNodes: TreeNodes
  private children: string[]

  constructor(
    private readonly original: TreeNodes,
    private readonly parentKey: string,
    private readonly parent: TreeNodeState,
  ) {
    this.treeNodes = original
    this.children = parent.children
  }

  nodeAt(path: string) {
    return treeNodeAtPath(this.treeNodes, path)
  }

  removeChild(path: string, pruneSubtree: boolean) {
    const childPath = childPathInList(this.children, path)
    if (childPath) {
      this.children = this.children.filter((candidate) => !pathsMatch(candidate, path))
    }
    if (pruneSubtree) {
      this.pruneSubtree(path, childPath !== null)
    }
  }

  upsertDirectory(entry: DirectoryEntry, previous: TreeNodeState | undefined) {
    const existingKey = treeNodeKeyForPath(this.treeNodes, entry.path)
    const existing = existingKey ? this.treeNodes[existingKey] : undefined
    const treeNodes = this.writableNodes()
    if (existingKey && existingKey !== entry.path) {
      delete treeNodes[existingKey]
    }
    treeNodes[entry.path] = {
      id: entry.path,
      name: entry.name,
      path: entry.path,
      parentPath: this.parent.path,
      ...inheritedTreeNodeState(previous, existing),
      lastAccess: Date.now(),
    }
    if (!childPathInList(this.children, entry.path)) {
      this.writableChildren().push(entry.path)
    }
  }

  result(): TreeNodes | null {
    if (this.children !== this.parent.children) {
      this.commitChildren()
    }
    return this.treeNodes === this.original ? null : this.treeNodes
  }

  private commitChildren() {
    const treeNodes = this.writableNodes()
    const uniqueChildren = Array.from(new Set(this.children))
    treeNodes[this.parentKey] = {
      ...this.parent,
      children: uniqueChildren.sort((left, right) => compareTreeChildPaths(treeNodes, left, right)),
    }
  }

  private pruneSubtree(path: string, wasListedChild: boolean) {
    const nodeKey = treeNodeKeyForPath(this.treeNodes, path)
    if (!nodeKey) {
      return
    }
    const node = this.treeNodes[nodeKey]
    const belongsToParent = pathsMatch(node.parentPath ?? '', this.parent.path)
    if (!belongsToParent && !wasListedChild) {
      return
    }
    const doomed = new Set<string>()
    collectTreeSubtreePaths(this.treeNodes, nodeKey, doomed)
    const treeNodes = this.writableNodes()
    for (const doomedPath of doomed) {
      delete treeNodes[doomedPath]
    }
  }

  private writableNodes() {
    if (this.treeNodes === this.original) {
      this.treeNodes = { ...this.original }
    }
    return this.treeNodes
  }

  private writableChildren() {
    if (this.children === this.parent.children) {
      this.children = [...this.parent.children]
    }
    return this.children
  }
}

export function applyTreeDirPatch(
  state: Pick<PanesData, 'treeNodes' | 'showHiddenFiles'>,
  event: DirPatchEvent,
): TreeNodes | null {
  const parentKey = treeNodeKeyForPath(state.treeNodes, event.path)
  if (!parentKey) {
    return null
  }
  const parent = state.treeNodes[parentKey]
  if (!parent.loaded) {
    return null
  }

  const draft = new TreePatchDraft(state.treeNodes, parentKey, parent)
  for (const path of event.removed) {
    draft.removeChild(path, true)
  }
  for (const change of event.changed) {
    const directory = visibleDirectory(change.entry, state.showHiddenFiles)
    const previous = draft.nodeAt(change.path)
    draft.removeChild(change.path, directory === null)
    if (directory) {
      draft.upsertDirectory(directory, previous)
    }
  }
  return draft.result()
}
