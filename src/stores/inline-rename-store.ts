import { create } from 'zustand'
import type { PaneId } from '@/types/pane'

export type InlineRenameState = {
  paneId: PaneId
  entryId: string
  path: string
  initialValue: string
  value: string
  busy: boolean
  error: string | null
}

export type EntryRenameProps = {
  isRenaming: boolean
  renameValue: string
  renameBusy: boolean
  renameError: string | null
}

const NOT_RENAMING: EntryRenameProps = {
  isRenaming: false,
  renameValue: '',
  renameBusy: false,
  renameError: null,
}

export function entryRenameProps(
  rename: InlineRenameState | null,
  entryId: string,
): EntryRenameProps {
  if (rename?.entryId !== entryId) {
    return NOT_RENAMING
  }
  return {
    isRenaming: true,
    renameValue: rename.value,
    renameBusy: rename.busy,
    renameError: rename.error,
  }
}

type BeginRenamePayload = {
  paneId: PaneId
  entryId: string
  path: string
  initialValue: string
}

type InlineRenameStore = {
  rename: InlineRenameState | null
  beginRename: (payload: BeginRenamePayload) => void
  setValue: (value: string) => void
  setBusy: (busy: boolean) => void
  setError: (error: string | null) => void
  cancelRename: () => void
  reset: () => void
}

export const useInlineRenameStore = create<InlineRenameStore>((set) => ({
  rename: null,
  beginRename: (payload) =>
    set({
      rename: {
        ...payload,
        value: payload.initialValue,
        busy: false,
        error: null,
      },
    }),
  setValue: (value) =>
    set((state) => ({
      rename: state.rename
        ? {
            ...state.rename,
            value,
          }
        : null,
    })),
  setBusy: (busy) =>
    set((state) => ({
      rename: state.rename
        ? {
            ...state.rename,
            busy,
          }
        : null,
    })),
  setError: (error) =>
    set((state) => ({
      rename: state.rename
        ? {
            ...state.rename,
            error,
          }
        : null,
    })),
  cancelRename: () => set({ rename: null }),
  reset: () => set({ rename: null }),
}))
