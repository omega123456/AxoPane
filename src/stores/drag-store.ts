import { create } from 'zustand'
import type { DragPayload } from '@/lib/drag-drop'

// Internal (webview) drag-and-drop shares a single active payload rather than
// serializing entries through `DataTransfer`: every drop target lives in the
// same React tree, so a store keeps the data typed and avoids round-tripping
// through strings. `DataTransfer` is still populated at drag start so the
// browser shows a drag image and reports a drop effect.
type DragStore = {
  drag: DragPayload | null
  begin: (payload: DragPayload) => void
  end: () => void
}

export const useDragStore = create<DragStore>((set) => ({
  drag: null,
  begin: (payload) => set({ drag: payload }),
  end: () => set({ drag: null }),
}))
