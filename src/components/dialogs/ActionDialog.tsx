import { useEffect, useRef, useState } from 'react'
import { Button, TextField } from '@/components/controls'
import {
  DialogError,
  DialogField,
  DialogList,
  DialogShell,
  DialogText,
} from '@/components/dialogs/DialogShell'
import { formatBytes } from '@/lib/format'
import { detectPlatformOs } from '@/lib/keymap'
import { log } from '@/lib/app-log-commands'
import { createFileInPane, createFolderInPane } from '@/lib/file-actions'
import { deleteFromTrash, emptyTrash, restartAsAdmin } from '@/lib/ipc/commands'
import { startOp } from '@/lib/queue-commands'
import {
  useActionDialogStore,
  type ActionDialog as ActionDialogState,
} from '@/stores/action-dialog-store'
import { usePanesStore } from '@/stores/panes-store'

export function ActionDialog() {
  const dialog = useActionDialogStore((state) => state.dialog)
  if (!dialog) {
    return null
  }

  if (dialog.kind === 'delete') {
    return <DeleteDialog dialog={dialog} />
  }

  if (dialog.kind === 'emptyTrash') {
    return <EmptyTrashDialog dialog={dialog} />
  }

  if (dialog.kind === 'calculateAllSizes') {
    return <CalculateAllSizesDialog dialog={dialog} />
  }

  if (dialog.kind === 'deleteFromTrash') {
    return <DeleteFromTrashDialog dialog={dialog} />
  }

  if (dialog.kind === 'adminRequired') {
    return <AdminRequiredDialog dialog={dialog} />
  }

  if (dialog.kind === 'transferConfirm') {
    return <TransferConfirmDialog dialog={dialog} />
  }

  if (dialog.kind === 'archiveConfirm') {
    return <ArchiveConfirmDialog dialog={dialog} />
  }

  return <PromptDialog dialog={dialog} />
}

const promptCopy = {
  newFolder: { title: 'New folder', label: 'Folder name', confirm: 'Create' },
  newFile: { title: 'New file', label: 'File name', confirm: 'Create' },
} as const

function pathSeparatorFor(path: string) {
  return path.includes('\\') || /^[A-Za-z]:/.test(path) ? '\\' : '/'
}

function joinPath(basePath: string, name: string) {
  const separator = pathSeparatorFor(basePath)
  const trimmedBase = /^[A-Za-z]:[\\/]+$/.test(basePath)
    ? `${basePath.slice(0, 2)}${separator}`
    : basePath.replace(/[\\/]+$/, '')
  if (trimmedBase === '') {
    return name
  }
  if (trimmedBase === '/' || /^[A-Za-z]:[\\/]$/.test(trimmedBase)) {
    return `${trimmedBase}${name}`
  }
  return `${trimmedBase}${separator}${name}`
}

/** Up to three "name · size" lines, then "and N more". */
function targetSummary(targets: Array<{ name: string; sizeBytes?: number | null }>) {
  const shown = targets
    .slice(0, 3)
    .map((target) =>
      target.sizeBytes == null ? target.name : `${target.name} · ${formatBytes(target.sizeBytes)}`,
    )
  return targets.length > 3 ? [...shown, `and ${targets.length - 3} more`] : shown
}

function archiveStem(targets: Array<{ name: string }>) {
  if (targets.length !== 1) {
    return 'Archive'
  }

  const name = targets[0].name.trim()
  const dotIndex = name.lastIndexOf('.')
  if (dotIndex > 0) {
    return name.slice(0, dotIndex)
  }
  return name || 'Archive'
}

function withZipExtension(path: string) {
  return path.toLowerCase().endsWith('.zip') ? path : `${path}.zip`
}

function PromptDialog({
  dialog,
}: {
  dialog: Extract<ActionDialogState, { kind: 'newFolder' | 'newFile' }>
}) {
  const close = useActionDialogStore((state) => state.close)
  const busy = useActionDialogStore((state) => state.busy)
  const error = useActionDialogStore((state) => state.error)
  const setBusy = useActionDialogStore((state) => state.setBusy)
  const setError = useActionDialogStore((state) => state.setError)
  const [value, setValue] = useState('')
  const inputRef = useRef<HTMLInputElement>(null)
  const copy = promptCopy[dialog.kind]

  useEffect(() => {
    inputRef.current?.focus()
    inputRef.current?.select()
  }, [])

  async function submit() {
    const trimmed = value.trim()
    if (!trimmed || busy) {
      return
    }

    setBusy(true)
    setError(null)
    try {
      if (dialog.kind === 'newFolder') {
        await createFolderInPane(dialog.paneId, trimmed)
      } else {
        await createFileInPane(dialog.paneId, trimmed)
      }
      close()
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause)
      log.error('file action failed', { kind: dialog.kind, error: message })
      setError(message)
      setBusy(false)
    }
  }

  function onKeyDown(event: React.KeyboardEvent) {
    if (event.key === 'Enter') {
      event.preventDefault()
      void submit()
    } else if (event.key === 'Escape') {
      event.preventDefault()
      close()
    }
  }

  return (
    <DialogShell
      label={copy.title}
      title={copy.title}
      onDismiss={close}
      footer={
        <>
          <Button
            variant="primary"
            disabled={busy || value.trim().length === 0}
            onClick={() => void submit()}
          >
            {copy.confirm}
          </Button>
          <Button onClick={close}>Cancel</Button>
        </>
      }
    >
      <DialogField label={copy.label} htmlFor="action-dialog-name">
        <TextField
          id="action-dialog-name"
          ref={inputRef}
          value={value}
          onChange={(event) => setValue(event.target.value)}
          onKeyDown={onKeyDown}
          aria-label={copy.label}
        />
      </DialogField>
      <DialogError message={error} />
    </DialogShell>
  )
}

function DeleteDialog({ dialog }: { dialog: Extract<ActionDialogState, { kind: 'delete' }> }) {
  const close = useActionDialogStore((state) => state.close)
  const busy = useActionDialogStore((state) => state.busy)
  const error = useActionDialogStore((state) => state.error)
  const setBusy = useActionDialogStore((state) => state.setBusy)
  const setError = useActionDialogStore((state) => state.setError)
  const confirmRef = useRef<HTMLButtonElement>(null)
  const count = dialog.targets.length

  useEffect(() => {
    confirmRef.current?.focus()
  }, [])

  async function confirm() {
    if (busy) {
      return
    }
    setBusy(true)
    setError(null)
    try {
      // Enqueue an irreversible delete through the shared queue engine so it
      // reuses the copy/move toast, progress and per-disk lock. Destination is
      // unused for deletes.
      await startOp({
        kind: 'delete',
        destinationDir: '',
        items: dialog.targets.map((target) => ({
          sourcePath: target.path,
          name: target.name,
          sizeBytes: target.sizeBytes ?? 0,
        })),
      })
      close()
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause)
      log.error('delete failed', { error: message })
      setError(message)
      setBusy(false)
    }
  }

  function onKeyDown(event: React.KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault()
      close()
    }
  }

  return (
    <DialogShell
      label="Confirm delete"
      title={`Delete ${count === 1 ? '1 item' : `${count} items`} permanently?`}
      onDismiss={close}
      onKeyDown={onKeyDown}
      footer={
        <>
          <Button variant="primary" ref={confirmRef} disabled={busy} onClick={() => void confirm()}>
            Delete
          </Button>
          <Button onClick={close}>Cancel</Button>
        </>
      }
    >
      <DialogText>
        {count === 1 ? 'This item skips' : 'These items skip'} the{' '}
        {detectPlatformOs() === 'macos' ? 'Trash' : 'Recycle Bin'}. You can't undo this.
      </DialogText>
      <DialogList items={targetSummary(dialog.targets)} />
      <DialogError message={error} />
    </DialogShell>
  )
}

/**
 * Windows refuses writes outside the user's own folders to a process that is
 * not elevated. The operation cannot be retried in place, so offer the only
 * fix: relaunch the application through the UAC prompt.
 */
function AdminRequiredDialog({
  dialog,
}: {
  dialog: Extract<ActionDialogState, { kind: 'adminRequired' }>
}) {
  const close = useActionDialogStore((state) => state.close)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const confirmRef = useRef<HTMLButtonElement>(null)

  useEffect(() => {
    confirmRef.current?.focus()
  }, [])

  async function confirm() {
    if (busy) {
      return
    }
    setBusy(true)
    setError(null)
    try {
      // Resolves only if the elevated instance started; this window is then
      // closed by the backend. A refused UAC prompt rejects instead.
      await restartAsAdmin()
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause)
      log.error('restart as administrator failed', { error: message })
      setError(message)
      setBusy(false)
    }
  }

  function onKeyDown(event: React.KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault()
      close()
    }
  }

  return (
    <DialogShell
      label="Administrator permissions needed"
      title="Administrator permissions needed"
      onDismiss={close}
      onKeyDown={onKeyDown}
      footer={
        <>
          <Button variant="primary" ref={confirmRef} disabled={busy} onClick={() => void confirm()}>
            Restart as administrator
          </Button>
          <Button onClick={close}>Cancel</Button>
        </>
      }
    >
      <DialogText>
        Windows refused this operation. Restart the application as an administrator, then try again.
      </DialogText>
      <DialogList items={[dialog.message]} />
      <DialogError message={error} />
    </DialogShell>
  )
}

function EmptyTrashDialog({
  dialog,
}: {
  dialog: Extract<ActionDialogState, { kind: 'emptyTrash' }>
}) {
  const close = useActionDialogStore((state) => state.close)
  const busy = useActionDialogStore((state) => state.busy)
  const error = useActionDialogStore((state) => state.error)
  const setBusy = useActionDialogStore((state) => state.setBusy)
  const setError = useActionDialogStore((state) => state.setError)
  const confirmRef = useRef<HTMLButtonElement>(null)
  const count = dialog.count

  useEffect(() => {
    confirmRef.current?.focus()
  }, [])

  async function confirm() {
    if (busy) {
      return
    }
    setBusy(true)
    setError(null)
    try {
      await emptyTrash()
      await usePanesStore.getState().reloadPane(dialog.paneId)
      close()
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause)
      log.error('empty trash failed', { error: message })
      setError(message)
      setBusy(false)
    }
  }

  function onKeyDown(event: React.KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault()
      close()
    }
  }

  return (
    <DialogShell
      label="Confirm empty trash"
      title={`Empty trash (${count === 1 ? '1 item' : `${count} items`})?`}
      onDismiss={close}
      onKeyDown={onKeyDown}
      footer={
        <>
          <Button variant="primary" ref={confirmRef} disabled={busy} onClick={() => void confirm()}>
            Empty trash
          </Button>
          <Button onClick={close}>Cancel</Button>
        </>
      }
    >
      <DialogText>
        This permanently deletes everything currently in the trash. You can't undo this.
      </DialogText>
      <DialogError message={error} />
    </DialogShell>
  )
}

function CalculateAllSizesDialog({
  dialog,
}: {
  dialog: Extract<ActionDialogState, { kind: 'calculateAllSizes' }>
}) {
  const close = useActionDialogStore((state) => state.close)
  const busy = useActionDialogStore((state) => state.busy)
  const error = useActionDialogStore((state) => state.error)
  const setBusy = useActionDialogStore((state) => state.setBusy)
  const setError = useActionDialogStore((state) => state.setError)
  const confirmRef = useRef<HTMLButtonElement>(null)

  useEffect(() => {
    confirmRef.current?.focus()
  }, [])

  async function confirm() {
    if (busy) {
      return
    }
    setBusy(true)
    setError(null)
    try {
      await usePanesStore.getState().calculateAllFolderSizes(dialog.paneId)
      close()
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause)
      log.error('calculate all folder sizes failed', { error: message })
      setError(message)
      setBusy(false)
    }
  }

  function onKeyDown(event: React.KeyboardEvent) {
    if (event.key === 'Enter') {
      event.preventDefault()
      void confirm()
    } else if (event.key === 'Escape') {
      event.preventDefault()
      close()
    }
  }

  return (
    <DialogShell
      label="Confirm calculate all folder sizes"
      title="Calculate all folder sizes?"
      onDismiss={close}
      onKeyDown={onKeyDown}
      footer={
        <>
          <Button variant="primary" ref={confirmRef} disabled={busy} onClick={() => void confirm()}>
            Calculate
          </Button>
          <Button onClick={close}>Cancel</Button>
        </>
      }
    >
      <DialogText>
        This scans every folder in this pane to compute its size. It may take a while for folders
        with many files.
      </DialogText>
      <DialogError message={error} />
    </DialogShell>
  )
}

function DeleteFromTrashDialog({
  dialog,
}: {
  dialog: Extract<ActionDialogState, { kind: 'deleteFromTrash' }>
}) {
  const close = useActionDialogStore((state) => state.close)
  const busy = useActionDialogStore((state) => state.busy)
  const error = useActionDialogStore((state) => state.error)
  const setBusy = useActionDialogStore((state) => state.setBusy)
  const setError = useActionDialogStore((state) => state.setError)
  const confirmRef = useRef<HTMLButtonElement>(null)
  const count = dialog.targets.length

  useEffect(() => {
    confirmRef.current?.focus()
  }, [])

  async function confirm() {
    if (busy) {
      return
    }
    setBusy(true)
    setError(null)
    try {
      await deleteFromTrash({ ids: dialog.targets.map((target) => target.id) })
      await usePanesStore.getState().reloadPane(dialog.paneId)
      close()
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause)
      log.error('delete from trash failed', { error: message })
      setError(message)
      setBusy(false)
    }
  }

  function onKeyDown(event: React.KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault()
      close()
    }
  }

  return (
    <DialogShell
      label="Confirm delete from trash"
      title={`Delete ${count === 1 ? '1 item' : `${count} items`} from trash?`}
      onDismiss={close}
      onKeyDown={onKeyDown}
      footer={
        <>
          <Button variant="primary" ref={confirmRef} disabled={busy} onClick={() => void confirm()}>
            Delete
          </Button>
          <Button onClick={close}>Cancel</Button>
        </>
      }
    >
      <DialogText>
        This permanently deletes the selected {count === 1 ? 'item' : 'items'} from the trash. You
        can't undo this.
      </DialogText>
      <DialogList items={targetSummary(dialog.targets)} />
      <DialogError message={error} />
    </DialogShell>
  )
}

function TransferConfirmDialog({
  dialog,
}: {
  dialog: Extract<ActionDialogState, { kind: 'transferConfirm' }>
}) {
  const close = useActionDialogStore((state) => state.close)
  const busy = useActionDialogStore((state) => state.busy)
  const error = useActionDialogStore((state) => state.error)
  const setBusy = useActionDialogStore((state) => state.setBusy)
  const setError = useActionDialogStore((state) => state.setError)
  const confirmRef = useRef<HTMLButtonElement>(null)
  const count = dialog.targets.length
  const actionLabel = dialog.operation === 'copy' ? 'Copy' : 'Move'

  useEffect(() => {
    confirmRef.current?.focus()
  }, [])

  async function confirm() {
    if (busy) {
      return
    }

    setBusy(true)
    setError(null)
    try {
      await startOp({
        kind: dialog.operation,
        destinationDir: dialog.destinationDir,
        items: dialog.targets.map((target) => ({
          sourcePath: target.path,
          name: target.name,
          sizeBytes: target.sizeBytes ?? 0,
        })),
      })
      close()
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause)
      log.error('transfer start failed', { operation: dialog.operation, error: message })
      setError(message)
      setBusy(false)
    }
  }

  function onKeyDown(event: React.KeyboardEvent) {
    if (event.key === 'Enter') {
      event.preventDefault()
      void confirm()
    } else if (event.key === 'Escape') {
      event.preventDefault()
      close()
    }
  }

  return (
    <DialogShell
      label={`Confirm ${dialog.operation}`}
      title={`${actionLabel} ${count === 1 ? '1 item' : `${count} items`} to the other pane?`}
      onDismiss={close}
      onKeyDown={onKeyDown}
      footer={
        <>
          <Button variant="primary" ref={confirmRef} disabled={busy} onClick={() => void confirm()}>
            {actionLabel}
          </Button>
          <Button onClick={close}>Cancel</Button>
        </>
      }
    >
      <DialogText>Review the transfer before it is added to the queue.</DialogText>
      <DialogList items={targetSummary(dialog.targets)} />
      <dl className="grid gap-2 text-caption">
        <div className="grid gap-0.5">
          <dt className="text-light-text-secondary dark:text-dark-text-secondary">From folder</dt>
          <dd className="break-all font-mono">{dialog.sourceDir}</dd>
        </div>
        <div className="grid gap-0.5">
          <dt className="text-light-text-secondary dark:text-dark-text-secondary">To folder</dt>
          <dd className="break-all font-mono">{dialog.destinationDir}</dd>
        </div>
      </dl>
      <DialogError message={error} />
    </DialogShell>
  )
}

function ArchiveConfirmDialog({
  dialog,
}: {
  dialog: Extract<ActionDialogState, { kind: 'archiveConfirm' }>
}) {
  const close = useActionDialogStore((state) => state.close)
  const busy = useActionDialogStore((state) => state.busy)
  const error = useActionDialogStore((state) => state.error)
  const setBusy = useActionDialogStore((state) => state.setBusy)
  const setError = useActionDialogStore((state) => state.setError)
  const confirmRef = useRef<HTMLButtonElement>(null)
  const defaultPath =
    dialog.operation === 'compress'
      ? joinPath(dialog.destinationDir, `${archiveStem(dialog.targets)}.zip`)
      : dialog.destinationDir
  const [destinationPath, setDestinationPath] = useState(defaultPath)
  const count = dialog.targets.length
  const actionLabel = dialog.operation === 'compress' ? 'Compress' : 'Extract'
  const title = `Confirm ${dialog.operation}`

  useEffect(() => {
    confirmRef.current?.focus()
  }, [])

  async function confirm() {
    const trimmedDestination = destinationPath.trim()
    if (busy || trimmedDestination.length === 0) {
      return
    }
    const queuedDestination =
      dialog.operation === 'compress' ? withZipExtension(trimmedDestination) : trimmedDestination

    setBusy(true)
    setError(null)
    try {
      await startOp({
        kind: dialog.operation,
        destinationDir: queuedDestination,
        items: dialog.targets.map((target) => ({
          sourcePath: target.path,
          name: target.name,
          sizeBytes: dialog.operation === 'extract' ? 0 : (target.sizeBytes ?? 0),
        })),
      })
      close()
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause)
      log.error('archive start failed', { operation: dialog.operation, error: message })
      setError(message)
      setBusy(false)
    }
  }

  function onKeyDown(event: React.KeyboardEvent) {
    if (event.key === 'Enter') {
      event.preventDefault()
      void confirm()
    } else if (event.key === 'Escape') {
      event.preventDefault()
      close()
    }
  }

  return (
    <DialogShell
      label={title}
      title={`${actionLabel} ${count === 1 ? '1 item' : `${count} items`}?`}
      onDismiss={close}
      onKeyDown={onKeyDown}
      footer={
        <>
          <Button
            variant="primary"
            ref={confirmRef}
            disabled={busy || destinationPath.trim().length === 0}
            onClick={() => void confirm()}
          >
            {actionLabel}
          </Button>
          <Button onClick={close}>Cancel</Button>
        </>
      }
    >
      <DialogText>
        {dialog.operation === 'compress'
          ? 'Choose the archive file to create before it is added to the queue.'
          : 'Choose the base folder before it is added to the queue.'}
      </DialogText>
      <DialogField
        label={dialog.operation === 'compress' ? 'Archive path' : 'Base folder'}
        htmlFor="archive-destination"
      >
        <TextField
          id="archive-destination"
          value={destinationPath}
          onChange={(event) => setDestinationPath(event.target.value)}
          className="font-mono"
        />
      </DialogField>
      <DialogError message={error} />
    </DialogShell>
  )
}
