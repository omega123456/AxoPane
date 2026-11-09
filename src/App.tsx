import { useEffect } from 'react'
import { ActionDialog } from '@/components/dialogs/ActionDialog'
import { ConflictDialog } from '@/components/dialogs/ConflictDialog'
import { DefaultAppDialog } from '@/components/dialogs/DefaultAppDialog'
import { PropertiesDialog } from '@/components/dialogs/PropertiesDialog'
import { SearchPalette } from '@/components/search/SearchPalette'
import { SettingsModal } from '@/components/dialogs/SettingsModal'
import { ContextMenu } from '@/components/menus/ContextMenu'
import { AppFrame } from '@/components/shell/AppFrame'
import { CommandBar } from '@/components/shell/CommandBar'
import { StatusBar } from '@/components/shell/StatusBar'
import { WorkspaceLayout } from '@/components/shell/WorkspaceLayout'
import { QueueOverlay } from '@/components/queue/QueueOverlay'
import { DragCursorBadge } from '@/components/states/DragCursorBadge'
import { ErrorToast } from '@/components/states/ErrorToast'
import { startAppBootstrap } from '@/lib/app-bootstrap'
import { useAppEventSubscriptions } from '@/lib/use-app-event-subscriptions'
import { UpdateBanner } from '@/components/states/UpdateBanner'
import { EverythingBanner } from '@/components/states/EverythingBanner'
import { useConfigStore } from '@/stores/config-store'
import { initializePanes, usePanesStore } from '@/stores/panes-store'
import { activeConflict, useQueueStore } from '@/stores/queue-store'
import { useThemeStore } from '@/stores/theme-store'

function App() {
  const treeRoots = usePanesStore((state) => state.treeRoots)
  const theme = useThemeStore((state) => state.theme)
  const applyTheme = useThemeStore((state) => state.setTheme)
  const persistThemePreference = useConfigStore((state) => state.setThemePreference)
  const conflict = useQueueStore(activeConflict)
  const resolveConflict = useQueueStore((state) => state.resolve)

  useEffect(() => startAppBootstrap(), [])

  useEffect(() => {
    void initializePanes()
  }, [treeRoots])

  useAppEventSubscriptions()

  return (
    <main className="flex h-full select-none flex-col overflow-hidden bg-light-base font-ui text-light-text dark:bg-dark-base dark:text-dark-text mica:bg-transparent">
      <AppFrame
        infoBars={
          <>
            <UpdateBanner />
            <EverythingBanner />
          </>
        }
        statusBar={<StatusBar />}
        overlay={
          <>
            <QueueOverlay />
            <ErrorToast />
            <DragCursorBadge />
          </>
        }
      >
        <WorkspaceLayout
          commandBar={
            <CommandBar
              theme={theme}
              setTheme={(nextTheme) => {
                applyTheme(nextTheme)
                void persistThemePreference(nextTheme)
              }}
            />
          }
        />
      </AppFrame>
      <ContextMenu />
      <SettingsModal />
      <ActionDialog />
      <PropertiesDialog />
      <DefaultAppDialog />
      <SearchPalette />
      {conflict ? (
        <ConflictDialog
          key={conflict.operationId}
          conflict={conflict}
          onResolve={(resolution, applyToAll, renameTo) =>
            resolveConflict({ id: conflict.operationId, resolution, applyToAll, renameTo })
          }
        />
      ) : null}
    </main>
  )
}

export default App
