import { afterEach, describe, expect, it, vi } from 'vitest'
import { canDropOnFavourite, dropOntoFavourite } from '@/components/tree/favourite-drop'
import { useConfigStore } from '@/stores/config-store'

const SLOT = { path: '/fav', index: 2 }
const ORIGINAL_CONFIG = useConfigStore.getState()

function fileTransfer(items: { path: string; isDir: boolean }[]) {
  return {
    kind: 'file-transfer' as const,
    sourcePaneId: 'left' as const,
    sourceDir: '/',
    items: items.map((item) => ({ ...item, id: item.path, name: item.path, sizeBytes: null })),
  }
}

describe('favourite drop', () => {
  afterEach(() => {
    useConfigStore.setState(ORIGINAL_CONFIG, true)
  })

  it('accepts favourites and transfers that carry a folder', () => {
    expect(canDropOnFavourite({ kind: 'favourite', path: '/a' })).toBe(true)
    expect(canDropOnFavourite(fileTransfer([{ path: '/d', isDir: true }]))).toBe(true)
    expect(canDropOnFavourite(fileTransfer([{ path: '/f', isDir: false }]))).toBe(false)
    expect(canDropOnFavourite(null)).toBe(false)
  })

  it('reorders a dragged favourite unless it is dropped on itself', async () => {
    const reorderFavourite = vi.fn(async () => undefined)
    useConfigStore.setState({ reorderFavourite })
    await dropOntoFavourite({ kind: 'favourite', path: '/a' }, SLOT)
    await dropOntoFavourite({ kind: 'favourite', path: '/fav' }, SLOT)
    expect(reorderFavourite).toHaveBeenCalledTimes(1)
    expect(reorderFavourite).toHaveBeenCalledWith('/a', 2)
  })

  it('adds dropped folders in order at the slot and ignores files', async () => {
    const addFavourite = vi.fn(async () => undefined)
    useConfigStore.setState({ addFavourite })
    const drag = fileTransfer([
      { path: '/one', isDir: true },
      { path: '/file', isDir: false },
      { path: '/two', isDir: true },
    ])
    await dropOntoFavourite(drag, SLOT)
    await dropOntoFavourite(null, SLOT)
    expect(addFavourite.mock.calls).toEqual([
      ['/one', 2],
      ['/two', 3],
    ])
  })
})
