import { hydrateAppConfig, persistAppConfig } from '@/lib/app-config'
import { executeCommand } from '@/lib/commands'
import { folderSizeIndexStatus, listVolumes, loadConfig, loadSession } from '@/lib/ipc/commands'
import { onSizeIndexStatus } from '@/lib/ipc/events'
import { resolveCommandForEvent } from '@/lib/keymap'
import { isEditableTarget } from '@/lib/editable-target'
import { otherPane } from '@/lib/pane-id'
import type { CommandId, PaneMode, Shortcut } from '@/lib/types/ipc'
import { usePanesStore } from '@/stores/panes-store'
import { initializeTheme, useThemeStore } from '@/stores/theme-store'
import { applyWindowBackdrop } from '@/lib/window-commands'
import type { PaneId } from '@/types/pane'
import { BACK_MOUSE_BUTTON, FORWARD_MOUSE_BUTTON } from '@/lib/mouse-buttons'

const TAB_KEY = 'Tab'
const DUAL_PANE_MODE: PaneMode = 'dual'

type HistoryNavigation = (paneId: PaneId) => Promise<void>

export type GlobalKeyContext = {
  modalOpen: boolean
  defaultPaneMode: PaneMode
  keymap: Record<CommandId, Shortcut[]>
}

const historyNavigationByMouseButton: Partial<Record<number, HistoryNavigation>> = {
  [BACK_MOUSE_BUTTON]: (paneId) => usePanesStore.getState().goBack(paneId),
  [FORWARD_MOUSE_BUTTON]: (paneId) => usePanesStore.getState().goForward(paneId),
}

function focusOtherPane() {
  const nextPaneId = otherPane(usePanesStore.getState().activePaneId)
  usePanesStore.getState().setActivePane(nextPaneId)
  document.querySelector<HTMLElement>(`[data-pane-id="${nextPaneId}"]`)?.focus()
}

async function loadInitialState(statusSubscribed: Promise<unknown>) {
  const [config, session, status, volumes] = await Promise.all([
    loadConfig(),
    loadSession(),
    statusSubscribed.then(() => folderSizeIndexStatus()),
    listVolumes(),
  ])

  const { config: hydratedConfig, migrated } = hydrateAppConfig(config)
  useThemeStore.getState().setThemePreference(hydratedConfig.theme)
  if (migrated) {
    await persistAppConfig()
  }

  const panes = usePanesStore.getState()
  panes.initialize({
    session,
    showHiddenFiles: hydratedConfig.showHiddenFiles,
    folderSizeIndexStatus: status,
    volumes,
  })
  await Promise.all([panes.reloadPane('left'), panes.reloadPane('right')])
}

export function startAppBootstrap() {
  initializeTheme()
  void applyWindowBackdrop()
  const unlistenStatusPromise = onSizeIndexStatus((status) =>
    usePanesStore.getState().setFolderSizeIndexStatus(status),
  )
  void loadInitialState(unlistenStatusPromise)
  return () => {
    void unlistenStatusPromise.then((unlisten) => unlisten())
  }
}

export function handleGlobalKeyDown(event: KeyboardEvent, context: GlobalKeyContext) {
  const keyBelongsElsewhere = context.modalOpen || isEditableTarget(event.target)
  if (keyBelongsElsewhere) {
    return
  }

  const switchesPane = event.key === TAB_KEY && context.defaultPaneMode === DUAL_PANE_MODE
  if (switchesPane) {
    event.preventDefault()
    focusOtherPane()
    return
  }

  const commandId = resolveCommandForEvent(event, context.keymap)
  if (!commandId) {
    return
  }
  event.preventDefault()
  executeCommand(commandId, usePanesStore.getState().activePaneId)
}

export function handleHistoryMouseUp(event: MouseEvent, modalOpen: boolean) {
  const navigateHistory = historyNavigationByMouseButton[event.button]
  if (!navigateHistory) {
    return
  }
  if (modalOpen) {
    return
  }
  event.preventDefault()
  void navigateHistory(usePanesStore.getState().activePaneId)
}
