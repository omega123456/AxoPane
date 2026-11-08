import { useState, type Dispatch, type DragEvent, type SetStateAction } from 'react'
import { log } from '@/lib/app-log-commands'
import { startNativeDrag } from '@/lib/ipc/commands'
import { beginNativeDragBridge, endNativeDragBridge } from '@/lib/native-drag-bridge'
import {
  canDropInto,
  dropModifiers,
  performDrop,
  resolveDropKind,
  setDropEffect,
  type DragItem,
} from '@/lib/drag-drop'
import type { PlatformOs } from '@/lib/keymap'
import type { DirectoryEntry } from '@/lib/types/ipc'
import { useDragStore } from '@/stores/drag-store'
import type { PaneId } from '@/types/pane'
import { directoryEntryById, dragItemsForRow } from './pane-drag-source'

type PaneDragSourceOptions = {
  paneId: PaneId
  path: string
  entries: DirectoryEntry[]
  selectedIds: string[]
  platform: PlatformOs
}

type DropTarget = { path: string; event: DragEvent<HTMLDivElement> }

function acceptableFileTransfer(path: string, platform: PlatformOs) {
  const drag = useDragStore.getState().drag
  if (drag?.kind !== 'file-transfer') {
    return null
  }
  return canDropInto(drag, path, platform) ? drag : null
}

function markDropEffect(target: DropTarget, sourceDir: string, platform: PlatformOs) {
  target.event.preventDefault()
  const modifiers = dropModifiers(target.event, platform)
  setDropEffect(
    target.event.dataTransfer,
    resolveDropKind(modifiers, sourceDir, target.path, platform),
  )
}

function handOffToNativeDrag(items: DragItem[], onEnd: () => void) {
  beginNativeDragBridge()
  void startNativeDrag({ paths: items.map((item) => item.path) })
    .catch((error: unknown) => {
      log.error('native drag failed', { error })
    })
    .finally(() => {
      endNativeDragBridge()
      onEnd()
    })
}

type DropInto = (targetPath: string, event: DragEvent<HTMLDivElement>) => Promise<void>

type RowDragContext = PaneDragSourceOptions & {
  clearDragState: () => void
  dropInto: DropInto
  setDropTargetEntryId: Dispatch<SetStateAction<string | null>>
}

function rowDragHandlers(context: RowDragContext) {
  const { paneId, path, entries, selectedIds, platform } = context
  return {
    handleRowDragStart: (entryId: string, event: DragEvent<HTMLDivElement>) => {
      event.preventDefault()
      const items = dragItemsForRow(entries, selectedIds, entryId)
      if (items.length === 0) {
        return
      }
      const begin = useDragStore.getState().begin
      begin({ kind: 'file-transfer', sourcePaneId: paneId, sourceDir: path, items })
      handOffToNativeDrag(items, context.clearDragState)
    },
    handleFolderDragOver: (entryId: string, event: DragEvent<HTMLDivElement>) => {
      const entry = directoryEntryById(entries, entryId)
      const drag = entry ? acceptableFileTransfer(entry.path, platform) : null
      if (!entry || !drag) {
        return
      }
      event.stopPropagation()
      markDropEffect({ path: entry.path, event }, drag.sourceDir, platform)
      context.setDropTargetEntryId(entry.id)
    },
    handleFolderDragLeave: (entryId: string) => {
      context.setDropTargetEntryId((current) => (current === entryId ? null : current))
    },
    handleFolderDrop: async (entryId: string, event: DragEvent<HTMLDivElement>) => {
      const entry = directoryEntryById(entries, entryId)
      if (!entry) {
        return
      }
      event.preventDefault()
      event.stopPropagation()
      await context.dropInto(entry.path, event)
    },
  }
}

type PaneDropContext = {
  path: string
  platform: PlatformOs
  dropInto: DropInto
  setIsPaneDropTarget: (isTarget: boolean) => void
}

function paneDropHandlers({ path, platform, dropInto, setIsPaneDropTarget }: PaneDropContext) {
  return {
    handlePaneDragOver: (event: DragEvent<HTMLDivElement>) => {
      const drag = acceptableFileTransfer(path, platform)
      if (!drag) {
        return
      }
      markDropEffect({ path, event }, drag.sourceDir, platform)
      setIsPaneDropTarget(true)
    },
    handlePaneDragLeave: (event: DragEvent<HTMLDivElement>) => {
      const movedOntoChild = event.currentTarget.contains(event.relatedTarget as Node | null)
      if (movedOntoChild) {
        return
      }
      setIsPaneDropTarget(false)
    },
    handlePaneDrop: async (event: DragEvent<HTMLDivElement>) => {
      event.preventDefault()
      await dropInto(path, event)
    },
  }
}

export function usePaneDragSource(options: PaneDragSourceOptions) {
  const endDrag = useDragStore((state) => state.end)
  const [dropTargetEntryId, setDropTargetEntryId] = useState<string | null>(null)
  const [isPaneDropTarget, setIsPaneDropTarget] = useState(false)

  function clearDragState() {
    endDrag()
    setDropTargetEntryId(null)
    setIsPaneDropTarget(false)
  }

  async function dropInto(targetPath: string, event: DragEvent<HTMLDivElement>) {
    const drag = useDragStore.getState().drag
    const modifiers = dropModifiers(event, options.platform)
    clearDragState()
    await performDrop(drag, targetPath, modifiers, options.platform)
  }

  return {
    dropTargetEntryId,
    isPaneDropTarget,
    clearDragState,
    ...rowDragHandlers({ ...options, clearDragState, dropInto, setDropTargetEntryId }),
    ...paneDropHandlers({
      path: options.path,
      platform: options.platform,
      dropInto,
      setIsPaneDropTarget,
    }),
  }
}
