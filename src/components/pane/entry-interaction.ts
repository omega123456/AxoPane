import type { DragEvent, MouseEvent } from 'react'
import { isMiddleClickOpen } from '@/lib/mouse-buttons'
import type { DirectoryEntry } from '@/lib/types/ipc'
import type { FileRowActions } from './FileRow'

export function entryInteractionProps(entry: DirectoryEntry, actions: FileRowActions) {
  return {
    onDragStart: (event: DragEvent<HTMLDivElement>) => actions.onDragStart(entry.id, event),
    onDragEnd: actions.onDragEnd,
    onDragEnter: (event: DragEvent<HTMLDivElement>) => actions.onDragEnter(entry.id, event),
    onDragOver: (event: DragEvent<HTMLDivElement>) => actions.onDragOver(entry.id, event),
    onDragLeave: () => actions.onDragLeave(entry.id),
    onDrop: (event: DragEvent<HTMLDivElement>) => actions.onDrop(entry.id, event),
    onMouseDown: actions.onPointerDown,
    onDoubleClick: (event: MouseEvent<HTMLDivElement>) =>
      actions.onActivate(entry.id, event.detail, event.timeStamp),
    onClick: (event: MouseEvent<HTMLDivElement>) => actions.onClick(entry.id, event),
    onContextMenu: (event: MouseEvent<HTMLDivElement>) => actions.onContextMenu(entry.id, event),
    onAuxClick: (event: MouseEvent<HTMLDivElement>) => {
      if (isMiddleClickOpen(event, entry)) {
        event.preventDefault()
        actions.onMiddleClick(entry.id)
      }
    },
  }
}
