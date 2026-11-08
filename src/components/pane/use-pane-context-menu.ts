import type { MouseEvent } from 'react'
import {
  buildContextMenuContent,
  describeMenuTarget,
  resolveMenuTarget,
} from '@/components/menus/menu-definitions'
import type { PlatformOs } from '@/lib/keymap'
import type { DirectoryEntry } from '@/lib/types/ipc'
import { useContextMenuStore } from '@/stores/context-menu-store'
import { usePanesStore } from '@/stores/panes-store'
import { useSelectionStore } from '@/stores/selection-store'
import type { PaneId } from '@/types/pane'

type PaneContextMenuOptions = {
  paneId: PaneId
  entries: DirectoryEntry[]
  selectedIds: string[]
  platform: PlatformOs
}

function claimContextMenuEvent(event: MouseEvent) {
  event.preventDefault()
  event.stopPropagation()
}

function selectEntryForMenu(paneId: PaneId, entry: DirectoryEntry, selectedIds: string[]) {
  if (selectedIds.includes(entry.id)) return
  useSelectionStore.getState().setSelection(paneId, [entry.id], entry.id, entry.id)
  usePanesStore.getState().setFocusedEntry(paneId, entry.id)
}

export function usePaneContextMenu({
  paneId,
  entries,
  selectedIds,
  platform,
}: PaneContextMenuOptions) {
  const openMenu = useContextMenuStore((state) => state.openMenu)
  const setActivePane = usePanesStore((state) => state.setActivePane)

  return function showMenu(event: MouseEvent, entryId?: string) {
    claimContextMenuEvent(event)
    setActivePane(paneId)
    const entry = entryId ? entries.find((candidate) => candidate.id === entryId) : undefined
    if (entry) selectEntryForMenu(paneId, entry, selectedIds)
    const target = resolveMenuTarget(paneId, entry)
    const { title, chip } = describeMenuTarget(target)
    openMenu({
      paneId,
      title,
      chip,
      x: event.clientX,
      y: event.clientY,
      ...buildContextMenuContent(paneId, target, platform),
      targetId: entryId,
    })
  }
}
