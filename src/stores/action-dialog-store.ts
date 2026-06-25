import { create } from 'zustand'
import type { PaneId } from '@/types/pane'

export type DeleteTarget = {
  id: string
  name: string
  path: string
}

export type ActionDialog =
  | { kind: 'rename'; paneId: PaneId; entryId: string; path: string; initialValue: string }
  | { kind: 'newFolder'; paneId: PaneId }
  | { kind: 'newFile'; paneId: PaneId }
  | { kind: 'delete'; paneId: PaneId; targets: DeleteTarget[] }

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
