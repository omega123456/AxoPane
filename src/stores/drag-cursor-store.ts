import { create } from 'zustand'
import type { DropKind } from '@/lib/drag-drop'

export type DragCursor = {
  x: number
  y: number
  kind: DropKind
}

type DragCursorState = {
  cursor: DragCursor | null
  setCursor: (cursor: DragCursor | null) => void
}

export const useDragCursorStore = create<DragCursorState>((set) => ({
  cursor: null,
  setCursor: (cursor) => {
    set({ cursor })
  },
}))
