import { useEffect, useMemo } from 'react'
import { ActionDialog } from '@/components/dialogs/ActionDialog'
import { SettingsModal } from '@/components/dialogs/SettingsModal'
import { ContextMenu } from '@/components/menus/ContextMenu'
import { AppFrame } from '@/components/shell/AppFrame'
import { CommandBar } from '@/components/shell/CommandBar'
import { StatusBar } from '@/components/shell/StatusBar'
import { DetailsPanel } from '@/components/details/DetailsPanel'
import { FilePane } from '@/components/pane/FilePane'
import { FolderTree } from '@/components/tree/FolderTree'
import { QueueOverlay } from '@/components/queue/QueueOverlay'
import { hydrateAppConfig } from '@/lib/app-config'
import { executeCommand } from '@/lib/commands'
import { everythingStatus, listVolumes, loadConfig, loadSession } from '@/lib/ipc/commands'
import { installCloseGuard } from '@/lib/close-guard'
import { onDirPatch, onSizeState, onVolumesChanged } from '@/lib/ipc/events'
import { resolveCommandForEvent } from '@/lib/keymap'
import { UpdateBanner } from '@/components/states/UpdateBanner'
import { checkForAppUpdate, summarizeUpdate } from '@/lib/updater'
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
  const detailsVisible = useLayoutStore((state) => state.detailsVisible)
  const defaultPaneMode = useLayoutStore((state) => state.defaultPaneMode)
  const activeSelection = useSelectionStore((state) => state.selections[activePaneId])
  const keymap = useKeymapStore((state) => state.bindings)
  const settingsOpen = useSettingsStore((state) => state.isOpen)
  const menuOpen = useContextMenuStore((state) => state.menu !== null)
  const actionDialogOpen = useActionDialogStore((state) => state.dialog !== null)

  useEffect(() => {
    initializeTheme()

    void (async () => {
      const [config, session, status, nextVolumes] = await Promise.all([
        loadConfig(),
        loadSession(),
        everythingStatus(),
        listVolumes(),
      ])

      hydrateAppConfig(config)
      syncThemePreference(config.theme)

      initialize({
        session,
        showHiddenFiles: config.showHiddenFiles,
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
    void checkForAppUpdate().then((update) => {
      if (update) {
        useUpdaterStore.getState().setAvailable(update, summarizeUpdate(update))
      }
    })
  }, [])

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

      const target = event.target
      if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement) {
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
  }, [actionDialogOpen, keymap, menuOpen, settingsOpen])

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
      volume: volumes.find((volume) =>
        (activePane.path ?? '').toLowerCase().startsWith(volume.mountRoot.toLowerCase()),
      ),
    }
  }, [activePane, activeSelection, volumes])

  return (
    <main className="flex h-screen flex-col overflow-hidden bg-light-window font-ui text-light-text dark:bg-dark-window dark:text-dark-text">
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
        <FolderTree />
        {defaultPaneMode === 'single' ? (
          <div className="min-h-0 flex-1">
            <FilePane paneId={activePaneId} />
          </div>
        ) : (
          <div className="grid min-h-0 flex-1 grid-cols-2 divide-x divide-light-border dark:divide-dark-border">
            <FilePane paneId="left" />
            <FilePane paneId="right" />
          </div>
        )}
        {detailsVisible ? <DetailsPanel paneId={activePaneId} /> : null}
      </AppFrame>
      <ContextMenu />
      <SettingsModal />
      <ActionDialog />
    </main>
  )
}

export default App
