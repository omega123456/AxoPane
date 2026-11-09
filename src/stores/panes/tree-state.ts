import type { TreeChildEntry, VolumeInfo } from '@/lib/types/ipc'
import { pathKey, pathsMatch } from '@/lib/path-compare'
import { formatVolumeTreeName, isPathInsideVolume, sortVolumesForTree } from '@/lib/volumes'
import { expandabilityOf, pruneTreeCache, type LazyTreeNode } from '@/stores/panes/tree-cache'
import { basenameFromPath, getParentPath } from '@/stores/panes/pane-paths'
import type { PanesData, TreeNodeState, TreeNodes } from '@/stores/panes/pane-state'

type TreeState = Pick<PanesData, 'treeNodes' | 'treeRoots'>

type ListedChildNode = { nodePath: string; node: TreeNodeState }

export const UNKNOWN_EXPANDABILITY = 'unknown' as const

type UnloadedTreeNodeSpec = Pick<TreeNodeState, 'path' | 'name' | 'parentPath' | 'expandability'>

function unloadedTreeNode(spec: UnloadedTreeNodeSpec): TreeNodeState {
  return {
    id: spec.path,
    name: spec.name,
    path: spec.path,
    parentPath: spec.parentPath,
    children: [],
    expanded: false,
    loaded: false,
    expandability: spec.expandability,
    lastAccess: Date.now(),
  }
}

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
    node: unloadedTreeNode({
      path: nodePath,
      name: entry.name,
      parentPath: listedParentPath,
      expandability: expandabilityOf(entry),
    }),
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

export type TreeLoadTarget = {
  path: string
  currentNode: TreeNodeState | undefined
}

export function loadedTreeNodes(
  state: TreeState,
  { path, currentNode }: TreeLoadTarget,
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
  { path, currentNode }: TreeLoadTarget,
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

type RevealTarget = {
  ancestors: Set<string>
  leaf: string
}

function expandedForReveal({ ancestors, leaf }: RevealTarget, nodePath: string, expanded: boolean) {
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
  const isSameLeaf = state.treeFold.leaf === leaf
  const treeFold = isSameLeaf
    ? state.treeFold
    : { leaf, revealedSiblings: [], showHiddenLevels: false }
  const treeNodes = { ...state.treeNodes }
  for (const [nodePath, node] of Object.entries(state.treeNodes)) {
    const expanded = expandedForReveal({ ancestors, leaf }, nodePath, node.expanded)
    if (expanded !== node.expanded) {
      treeNodes[nodePath] = { ...node, expanded }
    }
  }
  return { treeNodes, treeFold }
}

function placeholderTreeNode(path: string): TreeNodeState {
  return unloadedTreeNode({
    path,
    name: basenameFromPath(path),
    parentPath: getParentPath(path),
    expandability: UNKNOWN_EXPANDABILITY,
  })
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
