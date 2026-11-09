import { create } from 'zustand'
import type { DragPayload } from '@/lib/drag-drop'

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
