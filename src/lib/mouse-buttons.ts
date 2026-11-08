export const PRIMARY_MOUSE_BUTTON = 0
export const MIDDLE_MOUSE_BUTTON = 1
export const BACK_MOUSE_BUTTON = 3
export const FORWARD_MOUSE_BUTTON = 4

type ButtonEvent = { button: number }

type OpenableEntry = { isDir: boolean; trashId?: string }

export function isMiddleButton(event: ButtonEvent) {
  return event.button === MIDDLE_MOUSE_BUTTON
}

function isOpenableFolder(entry: OpenableEntry) {
  return entry.isDir && !entry.trashId
}

export function isMiddleClickOpen(event: ButtonEvent, entry: OpenableEntry) {
  return isMiddleButton(event) && isOpenableFolder(entry)
}
