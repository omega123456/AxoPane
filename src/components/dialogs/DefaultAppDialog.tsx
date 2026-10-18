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

/** Extracts the extension (without the leading dot) from a file name, e.g. "sql" for "query.sql". */
function extensionFromFileName(fileName: string): string {
  const dotIndex = fileName.lastIndexOf('.')
  return dotIndex > 0 ? fileName.slice(dotIndex + 1) : ''
}

type LoadState = 'loaded' | 'error'

export function DefaultAppDialog() {
  const dialog = useDefaultAppDialogStore((state) => state.dialog)
  const close = useDefaultAppDialogStore((state) => state.close)
  const [selectedBundlePath, setSelectedBundlePath] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [errorMessage, setErrorMessage] = useState<string | null>(null)
  const cancelRef = useRef<HTMLButtonElement>(null)
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

  // The app list is preloaded by the caller (see PropertiesDialog) before
  // this dialog is opened, so it can render fully populated immediately
  // instead of showing its own internal loading flash.
  const apps = dialog?.apps ?? []
  const loadState: LoadState = apps.length > 0 ? 'loaded' : 'error'

  useEffect(() => {
    if (dialog) {
      cancelRef.current?.focus()
    }
  }, [dialog])

  if (!dialog) {
    return null
  }

  function onKeyDown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault()
      close()
    }
  }

  function selectApp(bundlePath: string) {
    setSelectedBundlePath(bundlePath)
    setErrorMessage(null)
  }

  async function confirm() {
    const app = apps.find((candidate) => candidate.bundlePath === selectedBundlePath)
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
      {loadState === 'error' ? (
        <p className="text-body text-light-text-secondary dark:text-dark-text-secondary">
          No applications were found.
        </p>
      ) : null}
      {loadState === 'loaded' ? (
        <ul
          role="listbox"
          aria-label="Applications"
          className="grid max-h-72 gap-0.5 overflow-auto rounded-control border border-light-stroke bg-light-layer p-1 scrollbar-thin scrollbar-track-transparent scrollbar-thumb-light-text-tertiary dark:border-dark-stroke dark:bg-dark-layer dark:scrollbar-thumb-dark-text-tertiary"
        >
          {apps.map((app) => {
            const selected = app.bundlePath === selectedBundlePath
            return (
              <li key={app.bundlePath}>
                <button
                  type="button"
                  role="option"
                  aria-selected={selected}
                  onClick={() => selectApp(app.bundlePath)}
                  className={`flex h-8 w-full items-center gap-2 rounded-control px-2 text-left text-row text-light-text dark:text-dark-text ${FOCUS_RING_INSET} ${
                    selected
                      ? 'bg-light-tint dark:bg-dark-tint'
                      : 'hover:bg-light-hover dark:hover:bg-dark-hover'
                  }`}
                >
                  <EntryIcon
                    entry={{ name: app.name, isDir: false, iconDataUrl: app.iconDataUrl }}
                  />
                  <span className="truncate">{app.name}</span>
                </button>
              </li>
            )
          })}
        </ul>
      ) : null}
      {errorMessage ? <InfoBar severity="error">{errorMessage}</InfoBar> : null}
    </DialogShell>
  )
}
