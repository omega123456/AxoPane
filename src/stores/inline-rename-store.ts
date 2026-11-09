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

export type InlineRenameDraft = Pick<InlineRenameState, 'value' | 'busy' | 'error'>

export function renameDraftForEntry(
  rename: InlineRenameState | null,
  entryId: string,
): InlineRenameDraft | undefined {
  if (rename?.entryId !== entryId) {
    return undefined
  }
  return rename
}

type RenameHolder = { rename: InlineRenameState | null }

export function withRenameDraft(
  state: RenameHolder,
  draft: Partial<InlineRenameDraft>,
): RenameHolder {
  if (!state.rename) {
    return { rename: null }
  }
  return { rename: { ...state.rename, ...draft } }
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
  setValue: (value) => set((state) => withRenameDraft(state, { value })),
  setBusy: (busy) => set((state) => withRenameDraft(state, { busy })),
  setError: (error) => set((state) => withRenameDraft(state, { error })),
  cancelRename: () => set({ rename: null }),
  reset: () => set({ rename: null }),
}))
