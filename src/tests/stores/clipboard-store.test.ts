import { beforeEach } from 'vitest'
import { useClipboardStore } from '@/stores/clipboard-store'
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

    const state = useClipboardStore.getState()
    expect(state.mode).toBe('move')
    expect(state.sourcePaneId).toBeNull()
    expect(state.entries).toEqual([
      { path: '/fixture/reports/Report.txt', name: 'Report.txt', sizeBytes: null },
    ])
  })

  it('keeps the in-app clipboard when the OS reports the same paths', async () => {
    useClipboardStore
      .getState()
      .setClipboard('move', 'left', [
        { path: '/fixture/reports/Report.txt', name: 'Report.txt', sizeBytes: 12 },
      ])
    ipc.override('read_file_clipboard', {
      mode: 'copy',
      items: [{ path: '/fixture/reports/Report.txt', name: 'Report.txt' }],
    })

    await useClipboardStore.getState().syncFromOs()

    const state = useClipboardStore.getState()
    expect(state.mode).toBe('move')
    expect(state.sourcePaneId).toBe('left')
    expect(state.entries[0].sizeBytes).toBe(12)
  })

  it('clears the clipboard when the OS clipboard holds no files', async () => {
    useClipboardStore
      .getState()
      .setClipboard('copy', 'left', [
        { path: '/fixture/reports/Report.txt', name: 'Report.txt', sizeBytes: 12 },
      ])
    ipc.override('read_file_clipboard', { mode: 'copy', items: [] })

    await useClipboardStore.getState().syncFromOs()

    const state = useClipboardStore.getState()
    expect(state.mode).toBeNull()
    expect(state.entries).toEqual([])
  })

  it('leaves the clipboard alone when the read fails', async () => {
    useClipboardStore
      .getState()
      .setClipboard('copy', 'left', [
        { path: '/fixture/reports/Report.txt', name: 'Report.txt', sizeBytes: 12 },
      ])
    ipc.override('read_file_clipboard', () => {
      throw new Error('clipboard unavailable')
    })

    await useClipboardStore.getState().syncFromOs()

    expect(useClipboardStore.getState().entries).toHaveLength(1)
  })
})
