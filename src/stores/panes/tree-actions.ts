import { listTreeChildren } from '@/lib/ipc/commands'
import { isTrashPath } from '@/lib/trash'
import type { VolumeInfo } from '@/lib/types/ipc'
import { findVolumeForPath, isPathInsideVolume } from '@/lib/volumes'
import { panesStore } from '@/stores/panes/store-handle'
import {
  failedTreeNodes,
  loadedTreeNodes,
  prunedTreeNodes,
  revealChain,
  revealedTree,
  toggledTreeNode,
  withHiddenLevelsShown,
  withRevealedSiblings,
  withVolumes,
} from '@/stores/panes/tree-state'

export const treeLoads = {
  generation: 0,
  flights: new Map<string, Promise<void>>(),
}

async function loadTreeChildren(path: string) {
  const currentNode = panesStore().getState().treeNodes[path]
  const requestGeneration = treeLoads.generation
  const isSuperseded = () => requestGeneration !== treeLoads.generation
  try {
    const response = await listTreeChildren({
      path,
      showHidden: panesStore().getState().showHiddenFiles,
    })
    if (!isSuperseded()) {
      panesStore().setState((state) => ({
        treeNodes: loadedTreeNodes(state, path, currentNode, response.children),
      }))
    }
  } catch {
    if (!isSuperseded()) {
      panesStore().setState((state) => ({
        treeNodes: failedTreeNodes(state, path, currentNode),
      }))
    }
  } finally {
    panesStore().setState((state) => ({
      treeNodes: prunedTreeNodes(state.treeNodes, state, state.treeRoots),
    }))
  }
}

async function loadTreeChildrenOnce(path: string) {
  let settleFlight: () => void = () => undefined
  const flight = new Promise<void>((resolve) => {
    settleFlight = resolve
  })
  treeLoads.flights.set(path, flight)
  try {
    await loadTreeChildren(path)
  } finally {
    treeLoads.flights.delete(path)
    settleFlight()
  }
}

export function setVolumes(volumes: VolumeInfo[]) {
  treeLoads.generation += 1
  panesStore().setState((state) => withVolumes(state, volumes))
}

export async function ensureTreeChildren(path: string | null) {
  if (!path) {
    return
  }
  if (panesStore().getState().treeNodes[path]?.loaded) {
    return
  }
  const existingFlight = treeLoads.flights.get(path)
  if (existingFlight) {
    return existingFlight
  }
  await loadTreeChildrenOnce(path)
}

export async function revealPath(path: string | null) {
  if (!path) {
    return
  }
  const root = findVolumeForPath(path, panesStore().getState().volumes)?.mountRoot ?? null
  if (!root) {
    return
  }

  const chain = revealChain(path, root)
  for (const ancestor of chain.slice(0, -1)) {
    await panesStore().getState().ensureTreeChildren(ancestor)
  }
  panesStore().setState((state) => revealedTree(state, path, chain))
}

export async function toggleTreeNode(path: string) {
  if (!panesStore().getState().treeNodes[path]?.loaded) {
    await panesStore().getState().ensureTreeChildren(path)
  }
  panesStore().setState((state) => toggledTreeNode(state, path))
}

export function revealTreeSiblings(parentPath: string) {
  panesStore().setState((state) => withRevealedSiblings(state, parentPath) ?? state)
}

export function revealTreeHiddenLevels() {
  panesStore().setState((state) => withHiddenLevelsShown(state))
}

export async function initializePanes() {
  const { ensureTreeChildren, treeRoots, panes, activePaneId } = panesStore().getState()
  const activePath = panes[activePaneId].path

  if (!activePath || isTrashPath(activePath)) {
    return
  }

  const activeRoot = treeRoots.find((root) => isPathInsideVolume(activePath, root))
  if (!activeRoot) {
    return
  }

  await ensureTreeChildren(activeRoot)
}
