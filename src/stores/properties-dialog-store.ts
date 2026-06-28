import { create } from 'zustand'
import type { DirectoryEntry } from '@/lib/types/ipc'

export type PropertiesDialogItem = Pick<
  DirectoryEntry,
  | 'attributes'
  | 'createdAt'
  | 'id'
  | 'isDir'
  | 'isHidden'
  | 'isSystem'
  | 'itemCount'
  | 'modifiedAt'
  | 'name'
  | 'path'
  | 'sizeBytes'
  | 'typeLabel'
>

export type PropertiesDialogState = {
  items: PropertiesDialogItem[]
}

type Store = {
  dialog: PropertiesDialogState | null
  open: (dialog: PropertiesDialogState) => void
  close: () => void
}

export const usePropertiesDialogStore = create<Store>((set) => ({
  dialog: null,
  open: (dialog) => set({ dialog }),
  close: () => set({ dialog: null }),
}))
