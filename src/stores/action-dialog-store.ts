import { create } from 'zustand'
import type { OpKind } from '@/lib/types/ipc'
import type { PaneId } from '@/types/pane'

export type DeleteTarget = {
  id: string
  name: string
  path: string
  sizeBytes?: number | null
}

export type ActionDialog =
  | { kind: 'newFolder'; paneId: PaneId }
  | { kind: 'newFile'; paneId: PaneId }
  | { kind: 'delete'; paneId: PaneId; targets: DeleteTarget[] }
  | { kind: 'calculateAllSizes'; paneId: PaneId }
  | { kind: 'emptyTrash'; paneId: PaneId; count: number }
  | { kind: 'deleteFromTrash'; paneId: PaneId; targets: DeleteTarget[] }
  | {
      kind: 'archiveConfirm'
      paneId: PaneId
      operation: Extract<OpKind, 'compress' | 'extract'>
      destinationDir: string
      targets: DeleteTarget[]
    }
  | {
      kind: 'transferConfirm'
      paneId: PaneId
      operation: OpKind
      sourceDir: string
      destinationDir: string
      targets: DeleteTarget[]
    }

type ActionDialogStore = {
  dialog: ActionDialog | null
  busy: boolean
  error: string | null
  open: (dialog: ActionDialog) => void
  close: () => void
  setBusy: (busy: boolean) => void
  setError: (error: string | null) => void
}

export const useActionDialogStore = create<ActionDialogStore>((set) => ({
  dialog: null,
  busy: false,
  error: null,
  open: (dialog) => set({ dialog, busy: false, error: null }),
  close: () => set({ dialog: null, busy: false, error: null }),
  setBusy: (busy) => set({ busy }),
  setError: (error) => set({ error }),
}))
