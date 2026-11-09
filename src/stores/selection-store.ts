import { create } from 'zustand'
import type { PaneId } from '@/types/pane'

export type PaneSelection = {
  anchorId: string | null
  focusedId: string | null
  selectedIds: string[]
}

type SelectionStore = {
  selections: Record<PaneId, PaneSelection>
  setSelection: (paneId: PaneId, selection: PaneSelection) => void
  clearSelectionForPane: (paneId: PaneId) => void
  reset: () => void
}

export const EMPTY_SELECTION: PaneSelection = Object.freeze({
  anchorId: null,
  focusedId: null,
  selectedIds: [],
})

export function singleEntrySelection(entryId: string): PaneSelection {
  return { anchorId: entryId, focusedId: entryId, selectedIds: [entryId] }
}

const emptySelections = (): Record<PaneId, PaneSelection> => ({
  left: EMPTY_SELECTION,
  right: EMPTY_SELECTION,
})

export const useSelectionStore = create<SelectionStore>((set) => ({
  selections: emptySelections(),
  setSelection: (paneId, selection) =>
    set((state) => ({
      selections: {
        ...state.selections,
        [paneId]: selection,
      },
    })),
  clearSelectionForPane: (paneId) =>
    set((state) => ({
      selections: {
        ...state.selections,
        [paneId]: EMPTY_SELECTION,
      },
    })),
  reset: () => set({ selections: emptySelections() }),
}))

export function clearSelectionForPane(paneId: PaneId) {
  useSelectionStore.getState().clearSelectionForPane(paneId)
}
