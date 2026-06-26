import { useEffect, useMemo } from 'react'
import { ActionDialog } from '@/components/dialogs/ActionDialog'
import { SettingsModal } from '@/components/dialogs/SettingsModal'
import { ContextMenu } from '@/components/menus/ContextMenu'
import { AppFrame } from '@/components/shell/AppFrame'
import { CommandBar } from '@/components/shell/CommandBar'
import { StatusBar } from '@/components/shell/StatusBar'
import { WorkspaceLayout } from '@/components/shell/WorkspaceLayout'
import { QueueOverlay } from '@/components/queue/QueueOverlay'
import { hydrateAppConfig, persistAppConfig } from '@/lib/app-config'
import { executeCommand } from '@/lib/commands'
import { isPathInsideVolume } from '@/lib/volumes'
import { everythingStatus, listVolumes, loadConfig, loadSession } from '@/lib/ipc/commands'
import { installCloseGuard } from '@/lib/close-guard'
import { onDirPatch, onSizeState, onVolumesChanged } from '@/lib/ipc/events'
import { resolveCommandForEvent } from '@/lib/keymap'
import { UpdateBanner } from '@/components/states/UpdateBanner'
import { useUpdaterStore } from '@/stores/updater-store'
import { useActionDialogStore } from '@/stores/action-dialog-store'
import { useContextMenuStore } from '@/stores/context-menu-store'
import { useConfigStore } from '@/stores/config-store'
import { useLayoutStore } from '@/stores/layout-store'
import { useKeymapStore } from '@/stores/keymap-store'
import { initializePanes, usePanesStore } from '@/stores/panes-store'
import { useSettingsStore } from '@/stores/settings-store'
import { useSelectionStore } from '@/stores/selection-store'
import { initializeTheme, useThemeStore } from '@/stores/theme-store'

function isEditableTarget(target: EventTarget | null) {
  return (
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement ||
    (target instanceof HTMLElement && target.isContentEditable)
  )
}

function App() {
  const initialize = usePanesStore((state) => state.initialize)
  const setEverythingStatus = usePanesStore((state) => state.setEverythingStatus)
  const setVolumes = usePanesStore((state) => state.setVolumes)
  const applySizeState = usePanesStore((state) => state.applySizeState)
  const applyDirPatch = usePanesStore((state) => state.applyDirPatch)
  const reloadPane = usePanesStore((state) => state.reloadPane)
  const panes = usePanesStore((state) => state.panes)
  const activePaneId = usePanesStore((state) => state.activePaneId)
  const volumes = usePanesStore((state) => state.volumes)
  const activePane = panes[activePaneId]
  const theme = useThemeStore((state) => state.theme)
  const applyTheme = useThemeStore((state) => state.setTheme)
  const syncThemePreference = useThemeStore((state) => state.setThemePreference)
  const persistThemePreference = useConfigStore((state) => state.setThemePreference)
  const defaultPaneMode = useLayoutStore((state) => state.defaultPaneMode)
  const activeSelection = useSelectionStore((state) => state.selections[activePaneId])
  const keymap = useKeymapStore((state) => state.bindings)
  const settingsOpen = useSettingsStore((state) => state.isOpen)
  const menuOpen = useContextMenuStore((state) => state.menu !== null)
  const actionDialogOpen = useActionDialogStore((state) => state.dialog !== null)
  const updateCheckInterval = useConfigStore((state) => state.updateCheckInterval)

  useEffect(() => {
    initializeTheme()

    void (async () => {
      const [config, session, status, nextVolumes] = await Promise.all([
        loadConfig(),
        loadSession(),
        everythingStatus(),
        listVolumes(),
      ])

      const { config: hydratedConfig, migrated } = hydrateAppConfig(config)
      syncThemePreference(hydratedConfig.theme)

      if (migrated) {
        await persistAppConfig()
      }

      initialize({
        session,
        showHiddenFiles: hydratedConfig.showHiddenFiles,
        everythingStatus: status,
        volumes: nextVolumes,
      })

      await Promise.all([reloadPane('left'), reloadPane('right')])
    })()
  }, [initialize, reloadPane, syncThemePreference])

  useEffect(() => {
    void initializePanes()
  }, [])

  useEffect(() => {
    // Always checks once on launch, then schedules background polls at the
    // configured cadence. Re-runs when the cadence setting changes.
    useUpdaterStore.getState().startPeriodicCheck()
    return () => {
      useUpdaterStore.getState().stopPeriodicCheck()
    }
  }, [updateCheckInterval])

  useEffect(() => {
    let unlisten: (() => void) | undefined
    void installCloseGuard().then((dispose) => {
      unlisten = dispose
    })
    return () => {
      unlisten?.()
    }
  }, [])

  useEffect(() => {
    void everythingStatus().then(setEverythingStatus)
  }, [setEverythingStatus])

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (settingsOpen || menuOpen || actionDialogOpen) {
        return
      }

      if (isEditableTarget(event.target)) {
        return
      }

      if (event.key === 'Tab' && defaultPaneMode === 'dual') {
        event.preventDefault()
        const nextPaneId = usePanesStore.getState().activePaneId === 'left' ? 'right' : 'left'
        usePanesStore.getState().setActivePane(nextPaneId)
        document.querySelector<HTMLElement>(`[data-pane-id="${nextPaneId}"]`)?.focus()
        return
      }

      const commandId = resolveCommandForEvent(event, keymap)
      if (!commandId) {
        return
      }

      event.preventDefault()
      executeCommand(commandId, usePanesStore.getState().activePaneId)
    }

    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [actionDialogOpen, defaultPaneMode, keymap, menuOpen, settingsOpen])

  useEffect(() => {
    // Mouse buttons 3 (back) and 4 (forward) drive history navigation on the
    // active pane. A mousedown over a pane already sets it active, so by the
    // time we read `activePaneId` here it reflects the pane under the cursor.
    function onMouseUp(event: MouseEvent) {
      if (event.button !== 3 && event.button !== 4) {
        return
      }

      if (settingsOpen || menuOpen || actionDialogOpen) {
        return
      }

      event.preventDefault()
      const paneId = usePanesStore.getState().activePaneId
      if (event.button === 3) {
        void usePanesStore.getState().goBack(paneId)
      } else {
        void usePanesStore.getState().goForward(paneId)
      }
    }

    window.addEventListener('mouseup', onMouseUp)
    return () => window.removeEventListener('mouseup', onMouseUp)
  }, [actionDialogOpen, menuOpen, settingsOpen])

  useEffect(() => {
    const unlistenVolumesPromise = onVolumesChanged((event) => {
      setVolumes(event.volumes)
    })

    const unlistenSizesPromise = onSizeState((event) => {
      applySizeState(event)
    })

    const unlistenPatchesPromise = onDirPatch((event) => {
      applyDirPatch(event)
    })

    return () => {
      void unlistenVolumesPromise.then((unlisten) => unlisten())
      void unlistenSizesPromise.then((unlisten) => unlisten())
      void unlistenPatchesPromise.then((unlisten) => unlisten())
    }
  }, [applyDirPatch, applySizeState, setVolumes])

  const statusSummary = useMemo(() => {
    const selectionCount = activeSelection?.selectedIds.length ?? 0
    const focusedEntry = activePane.focusedEntryId
      ? activePane.entries.find((entry) => entry.id === activePane.focusedEntryId)
      : undefined

    return {
      itemCount: activePane.entries.length,
      selectionCount,
      focusedEntry,
      volume: volumes.find((volume) => isPathInsideVolume(activePane.path ?? '', volume.mountRoot)),
    }
  }, [activePane, activeSelection, volumes])

  return (
    <main className="flex h-full flex-col overflow-hidden bg-light-window font-ui text-light-text dark:bg-dark-window dark:text-dark-text">
      <UpdateBanner />
      <AppFrame
        commandBar={
          <CommandBar
            theme={theme}
            setTheme={(nextTheme) => {
              applyTheme(nextTheme)
              void persistThemePreference(nextTheme)
            }}
          />
        }
        statusBar={<StatusBar activePane={activePane} summary={statusSummary} />}
        overlay={<QueueOverlay />}
      >
        <WorkspaceLayout />
      </AppFrame>
      <ContextMenu />
      <SettingsModal />
      <ActionDialog />
    </main>
  )
}

export default App
