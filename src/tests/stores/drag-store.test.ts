import { beforeEach } from 'vitest'
import { useDragStore } from '@/stores/drag-store'
import type { DragPayload } from '@/lib/drag-drop'

const payload: DragPayload = {
  kind: 'file-transfer',
  sourcePaneId: 'left',
  sourceDir: 'C:\\root',
  items: [{ id: 'a', name: 'Alpha', path: 'C:\\root\\Alpha', isDir: false, sizeBytes: 1 }],
}

beforeEach(() => {
  useDragStore.getState().end()
})

describe('drag store', () => {
  it('starts with no active drag', () => {
    expect(useDragStore.getState().drag).toBeNull()
  })

  it('holds the active payload while a drag is in flight and clears it on end', () => {
    useDragStore.getState().begin(payload)
    expect(useDragStore.getState().drag).toEqual(payload)

    useDragStore.getState().end()
    expect(useDragStore.getState().drag).toBeNull()
  })
})
