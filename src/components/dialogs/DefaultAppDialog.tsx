import { useEffect, useRef, useState, type KeyboardEvent } from 'react'
import { DialogShell } from '@/components/dialogs/DialogShell'
import { EntryIcon } from '@/components/icons/EntryIcon'
import { XCircleIcon } from '@/components/icons'
import { setDefaultApplication } from '@/lib/app-picker-commands'
import { getDefaultAppErrorMessage } from '@/lib/default-app-errors'
import {
  useDefaultAppDialogStore,
  type DefaultAppDialogState,
} from '@/stores/default-app-dialog-store'

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
    <DialogShell label="Set Default Application" onDismiss={close} onKeyDown={onKeyDown}>
      <div className="border-b border-light-border px-4 py-3 dark:border-dark-border">
        <div className="text-sm font-semibold text-light-text dark:text-dark-text">
          Set Default Application
        </div>
        <div className="mt-1 break-all text-row text-light-text-muted dark:text-dark-text-muted">
          Choose the app to always open “{dialog.fileName}” files with.
        </div>
      </div>
      <div className="p-4">
        {loadState === 'error' ? (
          <p className="text-row text-light-text-muted dark:text-dark-text-muted">
            No applications were found.
          </p>
        ) : null}
        {loadState === 'loaded' ? (
          <ul
            role="listbox"
            aria-label="Applications"
            className="max-h-72 space-y-1 overflow-auto rounded-tab border border-light-border bg-light-window p-2 scrollbar-thin scrollbar-track-transparent scrollbar-thumb-light-text-faint dark:border-dark-border dark:bg-dark-window dark:scrollbar-thumb-dark-text-faint"
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
                    className={`flex w-full items-center gap-2 rounded-tab px-2 py-1.5 text-left text-row focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border ${
                      selected
                        ? 'bg-accent-blue-soft text-accent-blue-light dark:text-accent-blue'
                        : 'text-light-text-soft hover:bg-light-hover dark:text-dark-text-soft dark:hover:bg-dark-hover'
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
        {errorMessage ? (
          <div
            role="alert"
            aria-live="assertive"
            className="mt-3 flex items-start gap-2 rounded-tab border border-accent-red bg-accent-red-soft px-3 py-2"
          >
            <XCircleIcon className="mt-0.5 h-5 w-5 shrink-0 text-accent-red" />
            <p className="text-row text-light-text dark:text-dark-text">{errorMessage}</p>
          </div>
        ) : null}
      </div>
      <div className="flex justify-end gap-2 border-t border-light-border p-4 dark:border-dark-border">
        <button
          ref={cancelRef}
          type="button"
          onClick={close}
          className="rounded-md border border-light-border px-4 py-2 text-xs text-light-text-soft focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border hover:bg-light-hover dark:border-dark-border dark:text-dark-text-soft dark:hover:bg-dark-hover"
        >
          Cancel
        </button>
        <button
          type="button"
          disabled={busy || selectedBundlePath === null}
          onClick={() => void confirm()}
          className="rounded-md bg-accent-blue-soft px-4 py-2 text-xs font-semibold text-accent-blue-light focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border disabled:opacity-40 dark:text-accent-blue"
        >
          Change All…
        </button>
      </div>
    </DialogShell>
  )
}
