import { create } from 'zustand'

export type DefaultAppDialogState = {
  filePath: string
  fileName: string
}

type Store = {
  dialog: DefaultAppDialogState | null
  open: (dialog: DefaultAppDialogState) => void
  close: () => void
}

export const useDefaultAppDialogStore = create<Store>((set) => ({
  dialog: null,
  open: (dialog) => set({ dialog }),
  close: () => set({ dialog: null }),
}))
