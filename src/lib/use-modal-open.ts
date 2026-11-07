import { useActionDialogStore } from '@/stores/action-dialog-store'
import { useContextMenuStore } from '@/stores/context-menu-store'
import { useDefaultAppDialogStore } from '@/stores/default-app-dialog-store'
import { usePropertiesDialogStore } from '@/stores/properties-dialog-store'
import { activeConflict, useQueueStore } from '@/stores/queue-store'
import { useSearchPaletteStore } from '@/stores/search-palette-store'
import { useSettingsStore } from '@/stores/settings-store'

export function useModalOpen() {
  const settingsOpen = useSettingsStore((state) => state.isOpen)
  const actionDialogOpen = useActionDialogStore((state) => state.dialog !== null)
  const propertiesDialogOpen = usePropertiesDialogStore((state) => state.dialog !== null)
  const menuOpen = useContextMenuStore((state) => state.menu !== null)
  const defaultAppDialogOpen = useDefaultAppDialogStore((state) => state.dialog !== null)
  const searchPaletteOpen = useSearchPaletteStore((state) => state.isOpen)
  const conflictOpen = useQueueStore((state) => activeConflict(state) !== undefined)

  return (
    settingsOpen ||
    actionDialogOpen ||
    propertiesDialogOpen ||
    menuOpen ||
    defaultAppDialogOpen ||
    searchPaletteOpen ||
    conflictOpen
  )
}
