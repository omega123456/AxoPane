import { create } from 'zustand'
import type { DirectoryEntry, FileClipboardMode } from '@/lib/types/ipc'

type ClipboardStore = {
  mode: FileClipboardMode | null
  sourcePaneId: 'left' | 'right' | null
  entries: DirectoryEntry[]
  setClipboard: (
    mode: FileClipboardMode,
    sourcePaneId: 'left' | 'right',
    entries: DirectoryEntry[],
  ) => void
  clearClipboard: () => void
}

export const useClipboardStore = create<ClipboardStore>((set) => ({
  mode: null,
  sourcePaneId: null,
  entries: [],
  setClipboard: (mode, sourcePaneId, entries) => set({ mode, sourcePaneId, entries }),
  clearClipboard: () => set({ mode: null, sourcePaneId: null, entries: [] }),
}))
