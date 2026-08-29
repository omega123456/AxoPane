import { create } from 'zustand'
import { log } from '@/lib/app-log-commands'
import { readFileClipboard } from '@/lib/ipc/commands'
import type { FileClipboardMode } from '@/lib/types/ipc'

/** The subset of a listing row that a paste needs. External clipboard items
 * carry no listing identity, so the store keeps only these fields. */
export type ClipboardEntry = {
  path: string
  name: string
  sizeBytes: number | null
}

type ClipboardStore = {
  mode: FileClipboardMode | null
  sourcePaneId: 'left' | 'right' | null
  entries: ClipboardEntry[]
  setClipboard: (
    mode: FileClipboardMode,
    sourcePaneId: 'left' | 'right',
    entries: ClipboardEntry[],
  ) => void
  clearClipboard: () => void
  /** Adopt what another application put on the OS clipboard. */
  syncFromOs: () => Promise<void>
}

function samePaths(left: ClipboardEntry[], right: { path: string }[]) {
  if (left.length !== right.length) {
    return false
  }
  const known = new Set(left.map((entry) => entry.path))
  return right.every((item) => known.has(item.path))
}

export const useClipboardStore = create<ClipboardStore>((set, get) => ({
  mode: null,
  sourcePaneId: null,
  entries: [],
  setClipboard: (mode, sourcePaneId, entries) => set({ mode, sourcePaneId, entries }),
  clearClipboard: () => set({ mode: null, sourcePaneId: null, entries: [] }),
  syncFromOs: async () => {
    let clipboard
    try {
      clipboard = await readFileClipboard()
    } catch (error) {
      log.warn('read file clipboard failed', { error })
      return
    }

    // The app writes its own copy/cut to the OS clipboard, so the same paths
    // mean this is our own entry. Keep it: it knows the source pane, and macOS
    // reports every file list as a copy.
    if (samePaths(get().entries, clipboard.items)) {
      return
    }

    set({
      mode: clipboard.items.length > 0 ? clipboard.mode : null,
      sourcePaneId: null,
      entries: clipboard.items.map((item) => ({
        path: item.path,
        name: item.name,
        sizeBytes: null,
      })),
    })
  },
}))
