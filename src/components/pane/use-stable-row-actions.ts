import { useLayoutEffect, useMemo, useRef } from 'react'
import type { FileRowActions } from './FileRow'

export function useStableRowActions(latestActions: FileRowActions): FileRowActions {
  const latestRef = useRef(latestActions)
  useLayoutEffect(() => {
    latestRef.current = latestActions
  })

  return useMemo<FileRowActions>(
    () => ({
      onPointerDown: () => latestRef.current.onPointerDown(),
      onActivate: (entryId, clickCount, eventTimeStamp) =>
        latestRef.current.onActivate(entryId, clickCount, eventTimeStamp),
      onClick: (entryId, event) => latestRef.current.onClick(entryId, event),
      onContextMenu: (entryId, event) => latestRef.current.onContextMenu(entryId, event),
      onMiddleClick: (entryId) => latestRef.current.onMiddleClick(entryId),
      onDragStart: (entryId, event) => latestRef.current.onDragStart(entryId, event),
      onDragEnd: () => latestRef.current.onDragEnd(),
      onDragEnter: (entryId, event) => latestRef.current.onDragEnter(entryId, event),
      onDragOver: (entryId, event) => latestRef.current.onDragOver(entryId, event),
      onDragLeave: (entryId) => latestRef.current.onDragLeave(entryId),
      onDrop: (entryId, event) => latestRef.current.onDrop(entryId, event),
      onRenameChange: (value) => latestRef.current.onRenameChange(value),
      onRenameSubmit: () => latestRef.current.onRenameSubmit(),
      onRenameCancel: () => latestRef.current.onRenameCancel(),
      onRenameBlur: () => latestRef.current.onRenameBlur(),
    }),
    [],
  )
}
