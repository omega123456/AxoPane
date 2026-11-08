import { useState } from 'react'
import { TextField } from '@/components/controls'
import { DialogField, DialogList, DialogText } from '@/components/dialogs/DialogShell'
import { TEXT_SECONDARY } from '@/lib/class-names'
import { startOp } from '@/lib/queue-commands'
import type { ActionDialog as ActionDialogState } from '@/stores/action-dialog-store'
import { ConfirmDialogShell } from './ConfirmDialogShell'
import {
  archiveStem,
  itemCountLabel,
  joinPath,
  targetSummary,
  withZipExtension,
} from './action-dialog-paths'
import { useFocusOnMount } from '@/lib/use-focus-on-mount'
import { useActionDialogSubmit } from './use-action-dialog-submit'

type TransferConfirm = Extract<ActionDialogState, { kind: 'transferConfirm' }>
type ArchiveConfirm = Extract<ActionDialogState, { kind: 'archiveConfirm' }>

export function TransferConfirmDialog({ dialog }: { dialog: TransferConfirm }) {
  const { close, busy, error, run } = useActionDialogSubmit()
  const confirmRef = useFocusOnMount<HTMLButtonElement>()
  const actionLabel = dialog.operation === 'copy' ? 'Copy' : 'Move'

  function confirm() {
    const enqueueTransfer = () =>
      startOp({
        kind: dialog.operation,
        destinationDir: dialog.destinationDir,
        items: dialog.targets.map((target) => ({
          sourcePath: target.path,
          name: target.name,
          sizeBytes: target.sizeBytes ?? 0,
        })),
      })
    const failure = { message: 'transfer start failed', context: { operation: dialog.operation } }
    void run(enqueueTransfer, failure)
  }

  return (
    <ConfirmDialogShell
      label={`Confirm ${dialog.operation}`}
      title={`${actionLabel} ${itemCountLabel(dialog.targets.length)} to the other pane?`}
      confirmLabel={actionLabel}
      confirmRef={confirmRef}
      confirmDisabled={busy}
      confirmOnEnter
      error={error}
      onConfirm={confirm}
      onClose={close}
    >
      <DialogText>Review the transfer before it is added to the queue.</DialogText>
      <DialogList items={targetSummary(dialog.targets)} />
      <TransferFolders sourceDir={dialog.sourceDir} destinationDir={dialog.destinationDir} />
    </ConfirmDialogShell>
  )
}

function TransferFolders({
  sourceDir,
  destinationDir,
}: {
  sourceDir: string
  destinationDir: string
}) {
  return (
    <dl className="grid gap-2 text-caption">
      <div className="grid gap-0.5">
        <dt className={TEXT_SECONDARY}>From folder</dt>
        <dd className="break-all font-mono">{sourceDir}</dd>
      </div>
      <div className="grid gap-0.5">
        <dt className={TEXT_SECONDARY}>To folder</dt>
        <dd className="break-all font-mono">{destinationDir}</dd>
      </div>
    </dl>
  )
}

function defaultArchiveDestination(dialog: ArchiveConfirm) {
  if (dialog.operation !== 'compress') {
    return dialog.destinationDir
  }
  return joinPath(dialog.destinationDir, `${archiveStem(dialog.targets)}.zip`)
}

const archiveCopy = {
  compress: {
    actionLabel: 'Compress',
    description: 'Choose the archive file to create before it is added to the queue.',
    fieldLabel: 'Archive path',
  },
  extract: {
    actionLabel: 'Extract',
    description: 'Choose the base folder before it is added to the queue.',
    fieldLabel: 'Base folder',
  },
} as const

export function ArchiveConfirmDialog({ dialog }: { dialog: ArchiveConfirm }) {
  const { close, busy, error, run } = useActionDialogSubmit()
  const confirmRef = useFocusOnMount<HTMLButtonElement>()
  const [destinationPath, setDestinationPath] = useState(() => defaultArchiveDestination(dialog))
  const copy = archiveCopy[dialog.operation]
  const trimmedDestination = destinationPath.trim()

  function confirm() {
    if (busy || trimmedDestination.length === 0) {
      return
    }
    const enqueueArchive = () => startOp(archiveOperation(dialog, trimmedDestination))
    const failure = { message: 'archive start failed', context: { operation: dialog.operation } }
    void run(enqueueArchive, failure)
  }

  return (
    <ConfirmDialogShell
      label={`Confirm ${dialog.operation}`}
      title={`${copy.actionLabel} ${itemCountLabel(dialog.targets.length)}?`}
      confirmLabel={copy.actionLabel}
      confirmRef={confirmRef}
      confirmDisabled={busy || trimmedDestination.length === 0}
      confirmOnEnter
      error={error}
      onConfirm={confirm}
      onClose={close}
    >
      <DialogText>{copy.description}</DialogText>
      <DialogField label={copy.fieldLabel} htmlFor="archive-destination">
        <TextField
          id="archive-destination"
          value={destinationPath}
          onChange={(event) => setDestinationPath(event.target.value)}
          className="font-mono"
        />
      </DialogField>
    </ConfirmDialogShell>
  )
}

function archiveOperation(dialog: ArchiveConfirm, trimmedDestination: string) {
  const isCompress = dialog.operation === 'compress'
  return {
    kind: dialog.operation,
    destinationDir: isCompress ? withZipExtension(trimmedDestination) : trimmedDestination,
    items: dialog.targets.map((target) => ({
      sourcePath: target.path,
      name: target.name,
      sizeBytes: isCompress ? (target.sizeBytes ?? 0) : 0,
    })),
  }
}
