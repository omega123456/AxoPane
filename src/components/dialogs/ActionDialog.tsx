import { useEffect, useRef, useState } from 'react'
import { DialogShell } from '@/components/dialogs/DialogShell'
import { AlertTriangleIcon } from '@/components/icons'
import { log } from '@/lib/app-log-commands'
import { createFileInPane, createFolderInPane } from '@/lib/file-actions'
import { startOp } from '@/lib/queue-commands'
import { useActionDialogStore, type ActionDialog as ActionDialogState } from '@/stores/action-dialog-store'

export function ActionDialog() {
  const dialog = useActionDialogStore((state) => state.dialog)
  if (!dialog) {
    return null
  }

  if (dialog.kind === 'delete') {
    return <DeleteDialog dialog={dialog} />
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
    <DialogShell label={copy.title} onDismiss={close}>
      <div className="border-b border-light-border p-4 dark:border-dark-border">
        <div className="text-sm font-semibold text-light-text dark:text-dark-text">{copy.title}</div>
      </div>
      <div className="p-4">
        <label className="block text-uxs uppercase tracking-wide text-light-text-muted dark:text-dark-text-muted">
          {copy.label}
        </label>
        <input
          ref={inputRef}
          value={value}
          onChange={(event) => setValue(event.target.value)}
          onKeyDown={onKeyDown}
          aria-label={copy.label}
          className="mt-2 h-9 w-full select-text rounded-tab border border-accent-blue-border bg-light-window px-3 font-mono text-xs text-light-text outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:bg-dark-window dark:text-dark-text"
        />
        {error ? (
          <p className="mt-2 flex items-center gap-2 text-uxs text-accent-amber">
            <AlertTriangleIcon className="h-3.5 w-3.5 shrink-0" />
            {error}
          </p>
        ) : null}
      </div>
      <div className="flex justify-end gap-2 border-t border-light-border p-4 dark:border-dark-border">
        <button
          type="button"
          onClick={close}
          className="rounded-md border border-light-border px-4 py-2 text-xs text-light-text-soft focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border hover:bg-light-hover dark:border-dark-border dark:text-dark-text-soft dark:hover:bg-dark-hover"
        >
          Cancel
        </button>
        <button
          type="button"
          disabled={busy || value.trim().length === 0}
          onClick={() => void submit()}
          className="rounded-md bg-accent-blue-soft px-4 py-2 text-xs font-semibold text-accent-blue-light focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border disabled:opacity-40 dark:text-accent-blue"
        >
          {copy.confirm}
        </button>
      </div>
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
    <DialogShell label="Confirm delete" onDismiss={close} onKeyDown={onKeyDown}>
      <div className="flex items-start gap-3 border-b border-light-border p-4 dark:border-dark-border">
        <AlertTriangleIcon className="mt-0.5 h-5 w-5 shrink-0 text-accent-amber" />
        <div className="min-w-0">
          <div className="text-sm font-semibold text-light-text dark:text-dark-text">
            Delete {count === 1 ? '1 item' : `${count} items`}?
          </div>
          <div className="mt-1 break-all font-mono text-uxs text-light-text-muted dark:text-dark-text-muted">
            {count === 1 ? dialog.targets[0].name : `${dialog.targets[0].name} and ${count - 1} more`}
          </div>
        </div>
      </div>
      <div className="p-4">
        <p className="text-row text-light-text-soft dark:text-dark-text-soft">
          This permanently deletes the selected {count === 1 ? 'item' : 'items'}. This cannot be undone.
        </p>
        {error ? (
          <p className="mt-2 flex items-center gap-2 text-uxs text-accent-amber">
            <AlertTriangleIcon className="h-3.5 w-3.5 shrink-0" />
            {error}
          </p>
        ) : null}
      </div>
      <div className="flex justify-end gap-2 border-t border-light-border p-4 dark:border-dark-border">
        <button
          type="button"
          onClick={close}
          className="rounded-md border border-light-border px-4 py-2 text-xs text-light-text-soft focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border hover:bg-light-hover dark:border-dark-border dark:text-dark-text-soft dark:hover:bg-dark-hover"
        >
          Cancel
        </button>
        <button
          ref={confirmRef}
          type="button"
          disabled={busy}
          onClick={() => void confirm()}
          className="rounded-md bg-accent-red-soft px-4 py-2 text-xs font-semibold text-accent-red focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-red disabled:opacity-40"
        >
          Delete
        </button>
      </div>
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
  const previewTargets = dialog.targets.slice(0, 4)
  const remainingCount = dialog.targets.length - previewTargets.length
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
    <DialogShell label={`Confirm ${dialog.operation}`} onDismiss={close} onKeyDown={onKeyDown}>
      <div className="flex items-start gap-3 border-b border-light-border p-4 dark:border-dark-border">
        <AlertTriangleIcon className="mt-0.5 h-5 w-5 shrink-0 text-accent-amber" />
        <div className="min-w-0">
          <div className="text-sm font-semibold text-light-text dark:text-dark-text">
            {actionLabel} {count === 1 ? '1 item' : `${count} items`} to the other pane?
          </div>
          <p className="mt-1 text-row text-light-text-soft dark:text-dark-text-soft">
            Review the transfer before it is added to the queue.
          </p>
        </div>
      </div>
      <div className="space-y-4 p-4">
        <div>
          <div className="text-uxs uppercase tracking-wide text-light-text-muted dark:text-dark-text-muted">
            Items
          </div>
          <ul className="mt-2 space-y-1 rounded-tab border border-light-border bg-light-window p-3 font-mono text-uxs text-light-text dark:border-dark-border dark:bg-dark-window dark:text-dark-text">
            {previewTargets.map((target) => (
              <li key={target.id} className="break-all">
                {target.name}
              </li>
            ))}
            {remainingCount > 0 ? (
              <li className="text-light-text-muted dark:text-dark-text-muted">
                and {remainingCount} more
              </li>
            ) : null}
          </ul>
        </div>
        <div className="grid gap-3">
          <div>
            <div className="text-uxs uppercase tracking-wide text-light-text-muted dark:text-dark-text-muted">
              From folder
            </div>
            <div className="mt-1 break-all rounded-tab border border-light-border bg-light-window px-3 py-2 font-mono text-uxs text-light-text dark:border-dark-border dark:bg-dark-window dark:text-dark-text">
              {dialog.sourceDir}
            </div>
          </div>
          <div>
            <div className="text-uxs uppercase tracking-wide text-light-text-muted dark:text-dark-text-muted">
              To folder
            </div>
            <div className="mt-1 break-all rounded-tab border border-light-border bg-light-window px-3 py-2 font-mono text-uxs text-light-text dark:border-dark-border dark:bg-dark-window dark:text-dark-text">
              {dialog.destinationDir}
            </div>
          </div>
        </div>
        {error ? (
          <p className="flex items-center gap-2 text-uxs text-accent-amber">
            <AlertTriangleIcon className="h-3.5 w-3.5 shrink-0" />
            {error}
          </p>
        ) : null}
      </div>
      <div className="flex justify-end gap-2 border-t border-light-border p-4 dark:border-dark-border">
        <button
          type="button"
          onClick={close}
          className="rounded-md border border-light-border px-4 py-2 text-xs text-light-text-soft focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border hover:bg-light-hover dark:border-dark-border dark:text-dark-text-soft dark:hover:bg-dark-hover"
        >
          Cancel
        </button>
        <button
          ref={confirmRef}
          type="button"
          disabled={busy}
          onClick={() => void confirm()}
          className="rounded-md bg-accent-blue-soft px-4 py-2 text-xs font-semibold text-accent-blue-light focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border disabled:opacity-40 dark:text-accent-blue"
        >
          {actionLabel}
        </button>
      </div>
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
          sizeBytes: dialog.operation === 'extract' ? 0 : target.sizeBytes ?? 0,
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
    <DialogShell label={title} onDismiss={close} onKeyDown={onKeyDown}>
      <div className="flex items-start gap-3 border-b border-light-border p-4 dark:border-dark-border">
        <AlertTriangleIcon className="mt-0.5 h-5 w-5 shrink-0 text-accent-amber" />
        <div className="min-w-0">
          <div className="text-sm font-semibold text-light-text dark:text-dark-text">
            {actionLabel} {count === 1 ? '1 item' : `${count} items`}?
          </div>
          <p className="mt-1 text-row text-light-text-soft dark:text-dark-text-soft">
            {dialog.operation === 'compress'
              ? 'Choose the archive file to create before it is added to the queue.'
              : 'Choose the base folder before it is added to the queue.'}
          </p>
        </div>
      </div>
      <div className="space-y-4 p-4">
        <div>
          <label
            htmlFor="archive-destination"
            className="text-uxs uppercase tracking-wide text-light-text-muted dark:text-dark-text-muted"
          >
            {dialog.operation === 'compress' ? 'Archive path' : 'Base folder'}
          </label>
          <input
            id="archive-destination"
            value={destinationPath}
            onChange={(event) => setDestinationPath(event.target.value)}
            className="mt-1 h-9 w-full select-text rounded-tab border border-light-border bg-light-window px-3 font-mono text-uxs text-light-text outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:border-dark-border dark:bg-dark-window dark:text-dark-text"
          />
        </div>
        {error ? (
          <p className="flex items-center gap-2 text-uxs text-accent-amber">
            <AlertTriangleIcon className="h-3.5 w-3.5 shrink-0" />
            {error}
          </p>
        ) : null}
      </div>
      <div className="flex justify-end gap-2 border-t border-light-border p-4 dark:border-dark-border">
        <button
          type="button"
          onClick={close}
          className="rounded-md border border-light-border px-4 py-2 text-xs text-light-text-soft focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border hover:bg-light-hover dark:border-dark-border dark:text-dark-text-soft dark:hover:bg-dark-hover"
        >
          Cancel
        </button>
        <button
          ref={confirmRef}
          type="button"
          disabled={busy || destinationPath.trim().length === 0}
          onClick={() => void confirm()}
          className="rounded-md bg-accent-blue-soft px-4 py-2 text-xs font-semibold text-accent-blue-light focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border disabled:opacity-40 dark:text-accent-blue"
        >
          {actionLabel}
        </button>
      </div>
    </DialogShell>
  )
}
