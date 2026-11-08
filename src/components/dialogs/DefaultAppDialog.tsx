import { useEffect, useRef, useState, type KeyboardEvent } from 'react'
import { Button } from '@/components/controls'
import { DialogShell, DialogText } from '@/components/dialogs/DialogShell'
import { EntryIcon } from '@/components/icons/EntryIcon'
import { InfoBar } from '@/components/shell/InfoBar'
import { setDefaultApplication } from '@/lib/app-picker-commands'
import { getDefaultAppErrorMessage } from '@/lib/default-app-errors'
import {
  useDefaultAppDialogStore,
  type DefaultAppDialogState,
} from '@/stores/default-app-dialog-store'
import { FOCUS_RING_INSET } from '@/lib/focus-ring'
import { HOVER_ON_POINTER, SCROLLBAR_THIN } from '@/lib/class-names'
import { TEXT_SECONDARY } from '@/lib/class-names'
import { handleKeyByTable } from '@/lib/key-table'

/** Extracts the extension (without the leading dot) from a file name, e.g. "sql" for "query.sql". */
function extensionFromFileName(fileName: string): string {
  const dotIndex = fileName.lastIndexOf('.')
  return dotIndex > 0 ? fileName.slice(dotIndex + 1) : ''
}

type LoadState = 'loaded' | 'error'

type ApplicationChoice = NonNullable<DefaultAppDialogState>['apps'][number]

type AppListboxProps = {
  apps: ApplicationChoice[]
  selectedBundlePath: string | null
  onSelect: (bundlePath: string) => void
}

function AppListbox({ apps, selectedBundlePath, onSelect }: AppListboxProps) {
  return (
    <ul
      role="listbox"
      aria-label="Applications"
      className={`grid max-h-72 gap-0.5 overflow-auto rounded-control border border-light-stroke bg-light-layer p-1 ${SCROLLBAR_THIN} dark:border-dark-stroke dark:bg-dark-layer`}
    >
      {apps.map((app) => (
        <AppOption
          key={app.bundlePath}
          app={app}
          selected={app.bundlePath === selectedBundlePath}
          onSelect={onSelect}
        />
      ))}
    </ul>
  )
}

const SELECTED_OPTION_CLASS = 'bg-light-tint dark:bg-dark-tint'

function appOptionStateClass(selected: boolean) {
  return selected ? SELECTED_OPTION_CLASS : HOVER_ON_POINTER
}

function AppOption({
  app,
  selected,
  onSelect,
}: {
  app: ApplicationChoice
  selected: boolean
  onSelect: (bundlePath: string) => void
}) {
  return (
    <li>
      <button
        type="button"
        role="option"
        aria-selected={selected}
        onClick={() => onSelect(app.bundlePath)}
        className={`flex h-8 w-full items-center gap-2 rounded-control px-2 text-left text-row text-light-text dark:text-dark-text ${FOCUS_RING_INSET} ${appOptionStateClass(selected)}`}
      >
        <EntryIcon entry={{ name: app.name, isDir: false, iconDataUrl: app.iconDataUrl }} />
        <span className="truncate">{app.name}</span>
      </button>
    </li>
  )
}

function useDefaultAppChoice(dialog: DefaultAppDialogState | null, close: () => void) {
  const [selectedBundlePath, setSelectedBundlePath] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [errorMessage, setErrorMessage] = useState<string | null>(null)
  const [prevDialog, setPrevDialog] = useState<DefaultAppDialogState | null>(dialog)

  // Reset render-derived state synchronously when the dialog re-opens for a
  // different target, rather than in an effect (avoids a cascading-render
  // setState-in-effect, since this only fires when `dialog` identity changes).
  if (dialog !== prevDialog) {
    setPrevDialog(dialog)
    if (dialog) {
      setSelectedBundlePath(null)
      setErrorMessage(null)
    }
  }

  function selectApp(bundlePath: string) {
    setSelectedBundlePath(bundlePath)
    setErrorMessage(null)
  }

  async function confirm() {
    const app = dialog?.apps.find((candidate) => candidate.bundlePath === selectedBundlePath)
    if (!app || busy || !dialog) {
      return
    }

    setBusy(true)
    setErrorMessage(null)
    const status = await setDefaultApplication(dialog.filePath, app)
    setBusy(false)

    if (status.handled) {
      close()
      return
    }

    setErrorMessage(
      getDefaultAppErrorMessage(status.message, {
        appName: app.name,
        extension: extensionFromFileName(dialog.fileName),
      }),
    )
  }

  return { selectedBundlePath, busy, errorMessage, selectApp, confirm }
}

function useFocusWhileOpen(dialog: DefaultAppDialogState | null) {
  const cancelRef = useRef<HTMLButtonElement>(null)
  useEffect(() => {
    if (dialog) {
      cancelRef.current?.focus()
    }
  }, [dialog])
  return cancelRef
}

export function DefaultAppDialog() {
  const dialog = useDefaultAppDialogStore((state) => state.dialog)
  const close = useDefaultAppDialogStore((state) => state.close)
  const { selectedBundlePath, busy, errorMessage, selectApp, confirm } = useDefaultAppChoice(
    dialog,
    close,
  )
  const cancelRef = useFocusWhileOpen(dialog)

  if (!dialog) {
    return null
  }

  // The app list is preloaded by the caller (see PropertiesDialog) before
  // this dialog is opened, so it can render fully populated immediately
  // instead of showing its own internal loading flash.
  const apps = dialog.apps
  const loadState: LoadState = apps.length > 0 ? 'loaded' : 'error'

  function onKeyDown(event: KeyboardEvent) {
    handleKeyByTable(event, { Escape: close })
  }

  return (
    <DialogShell
      label="Set Default Application"
      title="Set Default Application"
      onDismiss={close}
      onKeyDown={onKeyDown}
      footer={
        <>
          <Button
            variant="primary"
            disabled={busy || selectedBundlePath === null}
            onClick={() => void confirm()}
          >
            Change All…
          </Button>
          <Button ref={cancelRef} onClick={close}>
            Cancel
          </Button>
        </>
      }
    >
      <DialogText>Choose the app to always open “{dialog.fileName}” files with.</DialogText>
      {loadState === 'loaded' ? (
        <AppListbox apps={apps} selectedBundlePath={selectedBundlePath} onSelect={selectApp} />
      ) : (
        <p className={`text-body ${TEXT_SECONDARY}`}>No applications were found.</p>
      )}
      {errorMessage ? <InfoBar severity="error">{errorMessage}</InfoBar> : null}
    </DialogShell>
  )
}
