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

/** Absolute paths handed to the OS drag session so other apps can receive them. */
export type StartNativeDragRequest = {
  paths: string[]
}

/**
 * Where the pointer is during a native drag, in CSS pixels relative to the
 * webview's top-left. The webview sees no DOM drag events for an OS-owned drag,
 * so this stream is what drives in-app drop targets.
 */
export type DragPositionEvent = {
  /** Physical pixels, relative to the window frame's top-left. */
  cursorX: number
  cursorY: number
  /** The window frame's physical size, used to derive the chrome offset. */
  frameWidth: number
  frameHeight: number
  /** Modifier keys held right now — the webview sees no key events mid-drag. */
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
