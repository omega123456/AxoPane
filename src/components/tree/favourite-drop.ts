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

async function addFoldersAtIndex(drag: FileTransferDragPayload, firstIndex: number) {
  const folders = drag.items.filter((item) => item.isDir)
  for (const [offset, folder] of folders.entries()) {
    await useConfigStore.getState().addFavourite(folder.path, firstIndex + offset)
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
    await addFoldersAtIndex(drag, slot.index)
  }
}

export async function dropAtFavouritesEnd(drag: DragPayload | null) {
  const endIndex = useConfigStore.getState().favourites.length
  if (drag?.kind === 'favourite') {
    await useConfigStore.getState().reorderFavourite(drag.path, endIndex)
    return
  }
  if (drag?.kind === 'file-transfer') {
    await addFoldersAtIndex(drag, endIndex)
  }
}
