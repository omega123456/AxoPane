import { create } from 'zustand'

export type DialogStore<DialogState> = {
  dialog: DialogState | null
  open: (dialog: DialogState) => void
  close: () => void
}

/** A store holding one optional dialog payload with `open`/`close` actions. */
export function createDialogStore<DialogState>() {
  return create<DialogStore<DialogState>>((set) => ({
    dialog: null,
    open: (dialog) => set({ dialog }),
    close: () => set({ dialog: null }),
  }))
}
