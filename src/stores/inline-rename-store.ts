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
