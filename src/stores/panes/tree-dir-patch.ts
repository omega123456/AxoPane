import type { DirectoryEntry, DirPatchEvent } from '@/lib/types/ipc'
import { pathsMatch } from '@/lib/path-compare'
import { basenameFromPath } from '@/stores/panes/pane-paths'
import type { PanesData, TreeNodeState, TreeNodes } from '@/stores/panes/pane-state'
import { NATURAL_NAME_ORDER, UNKNOWN_EXPANDABILITY } from '@/stores/panes/tree-state'

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
