import { create } from 'zustand'

type PaneId = 'left' | 'right'

type PaneSelection = {
  anchorId: string | null
  focusedId: string | null
  selectedIds: string[]
}

type SelectionStore = {
  selections: Record<PaneId, PaneSelection>
  setSelection: (
    paneId: PaneId,
    selectedIds: string[],
    anchorId: string | null,
    focusedId: string | null,
  ) => void
  clearSelectionForPane: (paneId: PaneId) => void
  reset: () => void
}

const emptySelection = (): PaneSelection => ({
  anchorId: null,
  focusedId: null,
  selectedIds: [],
})

export const useSelectionStore = create<SelectionStore>((set) => ({
  selections: {
    left: emptySelection(),
    right: emptySelection(),
  },
  setSelection: (paneId, selectedIds, anchorId, focusedId) =>
    set((state) => ({
      selections: {
        ...state.selections,
        [paneId]: {
          selectedIds,
          anchorId,
          focusedId,
        },
      },
    })),
  clearSelectionForPane: (paneId) =>
    set((state) => ({
      selections: {
        ...state.selections,
        [paneId]: emptySelection(),
      },
    })),
  reset: () =>
    set({
      selections: {
        left: emptySelection(),
        right: emptySelection(),
      },
    }),
}))

export function clearSelectionForPane(paneId: PaneId) {
  useSelectionStore.getState().clearSelectionForPane(paneId)
}
