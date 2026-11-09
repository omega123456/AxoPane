export type CreateEntryRequest = {
  parent: string
  name: string
}

export type RenameEntryRequest = {
  path: string
  newName: string
}

export type TrashEntriesRequest = {
  paths: string[]
}

export type TrashEntry = {
  id: string
  name: string
  originalPath: string | null
  sizeBytes: number | null
  isDir: boolean
  deletedAt: number | null
}

export type ListTrashResponse = {
  entries: TrashEntry[]
}

export type RestoreTrashRequest = {
  ids: string[]
}

export type DeleteFromTrashRequest = {
  ids: string[]
}

export type OpenPathRequest = {
  path: string
}

export type StartNativeDragRequest = {
  paths: string[]
}

export type DragPositionEvent = {
  cursorX: number
  cursorY: number
  frameWidth: number
  frameHeight: number
  ctrlKey: boolean
  shiftKey: boolean
  altKey: boolean
  metaKey: boolean
}

export type FileClipboardMode = 'copy' | 'move'

export type WriteFileClipboardRequest = {
  mode: FileClipboardMode
  paths: string[]
}

export type FileClipboardItem = {
  path: string
  name: string
}

export type ReadFileClipboardResponse = {
  mode: FileClipboardMode
  items: FileClipboardItem[]
}

export type MenuActionStatus = {
  handled: boolean
  message?: string | null
}

export type EjectVolumeRequest = {
  mountRoot: string
}
