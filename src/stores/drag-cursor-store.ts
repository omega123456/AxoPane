import { create } from 'zustand'
import type { DropKind } from '@/lib/drag-drop'

/**
 * Where the pointer is during an OS-owned drag, and what dropping there would do.
 *
 * Only populated while a native drag is in flight. The OS cursor badge cannot
 * carry this: it is fixed to Copy for the whole session (wry forces
 * `NSDragOperation::Copy` when the page accepts nothing), so copy-vs-move
 * feedback has to be drawn by the app itself.
 */
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
