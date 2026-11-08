import type { DragPayload, FavouriteDragPayload, FileTransferDragPayload } from '@/lib/drag-drop'
import { pathsMatch } from '@/lib/path-compare'
import { useConfigStore } from '@/stores/config-store'

export type FavouriteSlot = { path: string; index: number }

async function reorderOntoFavourite(drag: FavouriteDragPayload, slot: FavouriteSlot) {
  if (pathsMatch(drag.path, slot.path)) {
    return
  }
  await useConfigStore.getState().reorderFavourite(drag.path, slot.index)
}

async function addFoldersAtFavourite(drag: FileTransferDragPayload, slot: FavouriteSlot) {
  const folders = drag.items.filter((item) => item.isDir)
  for (const [offset, folder] of folders.entries()) {
    await useConfigStore.getState().addFavourite(folder.path, slot.index + offset)
  }
}

export function canDropOnFavourite(drag: DragPayload | null) {
  if (drag?.kind === 'favourite') {
    return true
  }
  return drag?.items.some((item) => item.isDir) === true
}

export async function dropOntoFavourite(drag: DragPayload | null, slot: FavouriteSlot) {
  if (drag?.kind === 'favourite') {
    await reorderOntoFavourite(drag, slot)
    return
  }
  if (drag?.kind === 'file-transfer') {
    await addFoldersAtFavourite(drag, slot)
  }
}
