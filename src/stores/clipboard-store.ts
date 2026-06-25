import { create } from 'zustand'
import type { DirectoryEntry } from '@/lib/types/ipc'

type ClipboardMode = 'copy' | 'move'

type ClipboardStore = {
  mode: ClipboardMode | null
  sourcePaneId: 'left' | 'right' | null
  entries: DirectoryEntry[]
  setClipboard: (mode: ClipboardMode, sourcePaneId: 'left' | 'right', entries: DirectoryEntry[]) => void
  clearClipboard: () => void
}

export const useClipboardStore = create<ClipboardStore>((set) => ({
  mode: null,
  sourcePaneId: null,
  entries: [],
  setClipboard: (mode, sourcePaneId, entries) => set({ mode, sourcePaneId, entries }),
  clearClipboard: () => set({ mode: null, sourcePaneId: null, entries: [] }),
}))
