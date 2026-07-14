import { useEffect } from 'react'
import { ActionDialog } from '@/components/dialogs/ActionDialog'
import { ConflictDialog } from '@/components/dialogs/ConflictDialog'
import { DefaultAppDialog } from '@/components/dialogs/DefaultAppDialog'
import { PropertiesDialog } from '@/components/dialogs/PropertiesDialog'
import { SettingsModal } from '@/components/dialogs/SettingsModal'
import { ContextMenu } from '@/components/menus/ContextMenu'
import { AppFrame } from '@/components/shell/AppFrame'
import { CommandBar } from '@/components/shell/CommandBar'
import { StatusBar } from '@/components/shell/StatusBar'
import { WorkspaceLayout } from '@/components/shell/WorkspaceLayout'
import { QueueOverlay } from '@/components/queue/QueueOverlay'
import { ErrorToast } from '@/components/states/ErrorToast'
import { hydrateAppConfig, persistAppConfig } from '@/lib/app-config'
import { executeCommand } from '@/lib/commands'
import { everythingStatus, listVolumes, loadConfig, loadSession } from '@/lib/ipc/commands'
import { installCloseGuard } from '@/lib/close-guard'
import {
  onDirSessionPatch,
  onIconState,
  onItemCountState,
  onSizeState,
  onThumbnailState,
  onVolumesChanged,
} from '@/lib/ipc/events'
import { resolveCommandForEvent } from '@/lib/keymap'
import { createRafBatcher } from '@/lib/raf-batcher'
import type {
  IconStateEvent,
  ItemCountEvent,
  SizeStateEvent,
  ThumbnailResultEvent,
} from '@/lib/types/ipc'
import { UpdateBanner } from '@/components/states/UpdateBanner'
import { useUpdaterStore } from '@/stores/updater-store'
import { useActionDialogStore } from '@/stores/action-dialog-store'
import { useContextMenuStore } from '@/stores/context-menu-store'
import { useConfigStore } from '@/stores/config-store'
import { useLayoutStore } from '@/stores/layout-store'
import { useKeymapStore } from '@/stores/keymap-store'
import { initializePanes, usePanesStore } from '@/stores/panes-store'
import { activeConflict, useQueueStore } from '@/stores/queue-store'
import { useSettingsStore } from '@/stores/settings-store'
import { usePropertiesDialogStore } from '@/stores/properties-dialog-store'
import { useDefaultAppDialogStore } from '@/stores/default-app-dialog-store'
import { initializeTheme, useThemeStore } from '@/stores/theme-store'
import { useThumbnailStore } from '@/stores/thumbnail-store'

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
  const treeRoots = usePanesStore((state) => state.treeRoots)
  const applySizeStates = usePanesStore((state) => state.applySizeStates)
  const applyIconStates = usePanesStore((state) => state.applyIconStates)
  const applyItemCountEvents = usePanesStore((state) => state.applyItemCountEvents)
  const applySessionPatch = usePanesStore((state) => state.applySessionPatch)
  const applyThumbnailResults = useThumbnailStore((state) => state.applyThumbnailResults)
  const reloadPane = usePanesStore((state) => state.reloadPane)
  const theme = useThemeStore((state) => state.theme)
  const applyTheme = useThemeStore((state) => state.setTheme)
  const syncThemePreference = useThemeStore((state) => state.setThemePreference)
  const persistThemePreference = useConfigStore((state) => state.setThemePreference)
  const defaultPaneMode = useLayoutStore((state) => state.defaultPaneMode)
  const keymap = useKeymapStore((state) => state.bindings)
  const settingsOpen = useSettingsStore((state) => state.isOpen)
  const menuOpen = useContextMenuStore((state) => state.menu !== null)
  const actionDialogOpen = useActionDialogStore((state) => state.dialog !== null)
  const propertiesDialogOpen = usePropertiesDialogStore((state) => state.dialog !== null)
  const defaultAppDialogOpen = useDefaultAppDialogStore((state) => state.dialog !== null)
  const updateCheckInterval = useConfigStore((state) => state.updateCheckInterval)
  const conflict = useQueueStore(activeConflict)
  const resolveConflict = useQueueStore((state) => state.resolve)

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
  }, [treeRoots])

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
      if (
        settingsOpen ||
        menuOpen ||
        actionDialogOpen ||
        propertiesDialogOpen ||
        defaultAppDialogOpen ||
        conflict !== undefined
      ) {
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
  }, [
    actionDialogOpen,
    conflict,
    defaultAppDialogOpen,
    defaultPaneMode,
    keymap,
    menuOpen,
    propertiesDialogOpen,
    settingsOpen,
  ])

  useEffect(() => {
    // Mouse buttons 3 (back) and 4 (forward) drive history navigation on the
    // active pane. A mousedown over a pane already sets it active, so by the
    // time we read `activePaneId` here it reflects the pane under the cursor.
    function onMouseUp(event: MouseEvent) {
      if (event.button !== 3 && event.button !== 4) {
        return
      }

      if (
        settingsOpen ||
        menuOpen ||
        actionDialogOpen ||
        propertiesDialogOpen ||
        defaultAppDialogOpen ||
        conflict !== undefined
      ) {
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
  }, [
    actionDialogOpen,
    conflict,
    defaultAppDialogOpen,
    menuOpen,
    propertiesDialogOpen,
    settingsOpen,
  ])

  useEffect(() => {
    // Icon/size events can arrive in bursts (e.g. loading a folder full of
    // executables/subfolders); coalesce each burst into a single batched
    // store update per animation frame instead of one `set` per event.
    const iconBatcher = createRafBatcher<IconStateEvent>((batch) => {
      applyIconStates(batch)
    })
    const sizeBatcher = createRafBatcher<SizeStateEvent>((batch) => {
      applySizeStates(batch)
    })
    const itemCountBatcher = createRafBatcher<ItemCountEvent>((batch) => {
      applyItemCountEvents(batch)
    })
    const thumbnailBatcher = createRafBatcher<ThumbnailResultEvent>((batch) => {
      applyThumbnailResults(batch)
    })
    const unlistenVolumesPromise = onVolumesChanged((event) => {
      setVolumes(event.volumes)
    })

    const unlistenSizesPromise = onSizeState((events) => {
      for (const event of events) {
        sizeBatcher.push(event)
      }
    })

    const unlistenIconsPromise = onIconState((events) => {
      for (const event of events) {
        iconBatcher.push(event)
      }
    })

    const unlistenItemCountsPromise = onItemCountState((event) => {
      itemCountBatcher.push(event)
    })

    const unlistenPatchesPromise = onDirSessionPatch((event) => {
      applySessionPatch(event)
    })

    const unlistenThumbnailsPromise = onThumbnailState((events) => {
      for (const event of events) {
        thumbnailBatcher.push(event)
      }
    })

    return () => {
      void unlistenVolumesPromise.then((unlisten) => unlisten())
      void unlistenSizesPromise.then((unlisten) => unlisten())
      void unlistenIconsPromise.then((unlisten) => unlisten())
      void unlistenItemCountsPromise.then((unlisten) => unlisten())
      void unlistenPatchesPromise.then((unlisten) => unlisten())
      void unlistenThumbnailsPromise.then((unlisten) => unlisten())
      // Flush any buffered events before tearing down so a burst that lands
      // right before unmount is not silently dropped, then cancel the
      // batchers so no further frame callback fires.
      iconBatcher.flush()
      iconBatcher.cancel()
      sizeBatcher.flush()
      sizeBatcher.cancel()
      itemCountBatcher.flush()
      itemCountBatcher.cancel()
      thumbnailBatcher.flush()
      thumbnailBatcher.cancel()
    }
  }, [
    applySessionPatch,
    applyIconStates,
    applyItemCountEvents,
    applySizeStates,
    applyThumbnailResults,
    setVolumes,
  ])

  return (
    <main className="flex h-full select-none flex-col overflow-hidden overscroll-x-none bg-light-window font-ui text-light-text dark:bg-dark-window dark:text-dark-text">
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
        statusBar={<StatusBar />}
        overlay={
          <>
            <QueueOverlay />
            <ErrorToast />
          </>
        }
      >
        <WorkspaceLayout />
      </AppFrame>
      <ContextMenu />
      <SettingsModal />
      <ActionDialog />
      <PropertiesDialog />
      <DefaultAppDialog />
      {conflict ? (
        <ConflictDialog
          key={conflict.operationId}
          conflict={conflict}
          onResolve={(resolution, applyToAll, renameTo) =>
            resolveConflict(conflict.operationId, resolution, applyToAll, renameTo)
          }
        />
      ) : null}
    </main>
  )
}

export default App
