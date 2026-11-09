import { beforeEach } from 'vitest'
import {
  clipboardEntries,
  hasClipboardEntries,
  snapshotFromOs,
  useClipboardStore,
} from '@/stores/clipboard-store'
import { ipc } from '../ipc-mock'

beforeEach(() => {
  useClipboardStore.getState().clearClipboard()
})

describe('clipboard-store', () => {
  it('adopts the file paths another application put on the OS clipboard', async () => {
    ipc.override('read_file_clipboard', {
      mode: 'move',
      items: [{ path: '/fixture/reports/Report.txt', name: 'Report.txt' }],
    })

    await useClipboardStore.getState().syncFromOs()

    const state = useClipboardStore.getState().snapshot
    expect(state?.mode).toBe('move')
    expect(state?.sourcePaneId).toBeNull()
    expect(state?.entries).toEqual([
      { path: '/fixture/reports/Report.txt', name: 'Report.txt', sizeBytes: null },
    ])
  })

  it('keeps the in-app clipboard when the OS reports the same paths', async () => {
    useClipboardStore.getState().setClipboard({
      mode: 'move',
      sourcePaneId: 'left',
      entries: [{ path: '/fixture/reports/Report.txt', name: 'Report.txt', sizeBytes: 12 }],
    })
    ipc.override('read_file_clipboard', {
      mode: 'copy',
      items: [{ path: '/fixture/reports/Report.txt', name: 'Report.txt' }],
    })

    await useClipboardStore.getState().syncFromOs()

    const state = useClipboardStore.getState().snapshot
    expect(state?.mode).toBe('move')
    expect(state?.sourcePaneId).toBe('left')
    expect(state?.entries[0].sizeBytes).toBe(12)
  })

  it('clears the clipboard when the OS clipboard holds no files', async () => {
    useClipboardStore.getState().setClipboard({
      mode: 'copy',
      sourcePaneId: 'left',
      entries: [{ path: '/fixture/reports/Report.txt', name: 'Report.txt', sizeBytes: 12 }],
    })
    ipc.override('read_file_clipboard', { mode: 'copy', items: [] })

    await useClipboardStore.getState().syncFromOs()

    expect(useClipboardStore.getState().snapshot).toBeNull()
  })

  it('leaves the clipboard alone when the read fails', async () => {
    useClipboardStore.getState().setClipboard({
      mode: 'copy',
      sourcePaneId: 'left',
      entries: [{ path: '/fixture/reports/Report.txt', name: 'Report.txt', sizeBytes: 12 }],
    })
    ipc.override('read_file_clipboard', () => {
      throw new Error('clipboard unavailable')
    })

    await useClipboardStore.getState().syncFromOs()

    expect(clipboardEntries(useClipboardStore.getState().snapshot)).toHaveLength(1)
  })

  it('treats a missing snapshot as an empty clipboard', () => {
    expect(clipboardEntries(null)).toEqual([])
    expect(hasClipboardEntries()).toBe(false)
    useClipboardStore.getState().setClipboard({
      mode: 'copy',
      sourcePaneId: 'right',
      entries: [{ path: '/a', name: 'a', sizeBytes: 1 }],
    })
    expect(hasClipboardEntries()).toBe(true)
  })

  it('builds a snapshot from OS items, or none when the OS holds no files', () => {
    expect(snapshotFromOs({ mode: 'copy', items: [] })).toBeNull()
    expect(snapshotFromOs({ mode: 'move', items: [{ path: '/a', name: 'a' }] })).toEqual({
      mode: 'move',
      sourcePaneId: null,
      entries: [{ path: '/a', name: 'a', sizeBytes: null }],
    })
  })
})
