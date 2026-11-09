import { create } from 'zustand'
import { log } from '@/lib/app-log-commands'
import { readFileClipboard } from '@/lib/ipc/commands'
import type { FileClipboardMode, ReadFileClipboardResponse } from '@/lib/types/ipc'
import type { PaneId } from '@/types/pane'

export type ClipboardEntry = {
  path: string
  name: string
  sizeBytes: number | null
}

export type ClipboardSnapshot = {
  mode: FileClipboardMode
  sourcePaneId: PaneId | null
  entries: ClipboardEntry[]
}

type ClipboardStore = {
  snapshot: ClipboardSnapshot | null
  setClipboard: (snapshot: ClipboardSnapshot) => void
  clearClipboard: () => void
  syncFromOs: () => Promise<void>
}

const NO_CLIPBOARD_ENTRIES: ClipboardEntry[] = []

export function clipboardEntries(snapshot: ClipboardSnapshot | null): ClipboardEntry[] {
  return snapshot?.entries ?? NO_CLIPBOARD_ENTRIES
}

export function hasClipboardEntries(): boolean {
  return clipboardEntries(useClipboardStore.getState().snapshot).length > 0
}

function samePaths(left: ClipboardEntry[], right: { path: string }[]) {
  if (left.length !== right.length) {
    return false
  }
  const known = new Set(left.map((entry) => entry.path))
  return right.every((item) => known.has(item.path))
}

export function snapshotFromOs(clipboard: ReadFileClipboardResponse): ClipboardSnapshot | null {
  const hasItems = clipboard.items.length > 0
  if (!hasItems) {
    return null
  }
  return {
    mode: clipboard.mode,
    sourcePaneId: null,
    entries: clipboard.items.map((item) => ({
      path: item.path,
      name: item.name,
      sizeBytes: null,
    })),
  }
}

export const useClipboardStore = create<ClipboardStore>((set, get) => ({
  snapshot: null,
  setClipboard: (snapshot) => set({ snapshot }),
  clearClipboard: () => set({ snapshot: null }),
  syncFromOs: async () => {
    let clipboard
    try {
      clipboard = await readFileClipboard()
    } catch (error) {
      log.warn('read file clipboard failed', { error })
      return
    }

    if (samePaths(clipboardEntries(get().snapshot), clipboard.items)) {
      return
    }

    set({ snapshot: snapshotFromOs(clipboard) })
  },
}))
