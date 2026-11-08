import type { ReactNode, Ref } from 'react'
import { Button } from '@/components/controls'
import { DialogError, DialogShell } from '@/components/dialogs/DialogShell'
import { handleKeyByTable } from '@/lib/key-table'

export type ConfirmDialogShellProps = {
  label: string
  title: string
  confirmLabel: string
  confirmRef?: Ref<HTMLButtonElement>
  confirmDisabled: boolean
  confirmOnEnter?: boolean
  error: string | null
  onConfirm: () => void
  onClose: () => void
  children?: ReactNode
}

export function ConfirmDialogShell({
  label,
  title,
  confirmLabel,
  confirmRef,
  confirmDisabled,
  confirmOnEnter = false,
  error,
  onConfirm,
  onClose,
  children,
}: ConfirmDialogShellProps) {
  function onKeyDown(event: React.KeyboardEvent) {
    const enterHandlers = confirmOnEnter ? { Enter: onConfirm } : {}
    handleKeyByTable(event, { ...enterHandlers, Escape: onClose })
  }

  return (
    <DialogShell
      label={label}
      title={title}
      onDismiss={onClose}
      onKeyDown={onKeyDown}
      footer={
        <>
          <Button variant="primary" ref={confirmRef} disabled={confirmDisabled} onClick={onConfirm}>
            {confirmLabel}
          </Button>
          <Button onClick={onClose}>Cancel</Button>
        </>
      }
    >
      {children}
      <DialogError message={error} />
    </DialogShell>
  )
}
