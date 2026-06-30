import { waitFor } from '@testing-library/react'
import { beforeEach, vi } from 'vitest'
import { dispatchContextMenuAction } from '@/lib/context-menu/context-menu-actions'
import { ipc } from '@/tests/ipc-mock'
import { useActionDialogStore } from '@/stores/action-dialog-store'
import { usePanesStore } from '@/stores/panes-store'
import { usePropertiesDialogStore } from '@/stores/properties-dialog-store'
import type { DirectoryEntry } from '@/lib/types/ipc'

function entry(overrides: Partial<DirectoryEntry> = {}): DirectoryEntry {
  return {
    id: 'report',
    name: 'Report.txt',
    path: 'C:\\root\\Report.txt',
    isDir: false,
    sizeBytes: 2048,
    itemCount: null,
    typeLabel: 'TXT file',
    modifiedAt: '2026-06-20T10:15:00Z',
    createdAt: '2026-06-01T10:15:00Z',
    attributes: ['Archive'],
    isHidden: false,
    isSystem: false,
    ...overrides,
  }
}

beforeEach(() => {
  ipc.install()
  useActionDialogStore.getState().close()
  usePanesStore.getState().reset()
  usePropertiesDialogStore.getState().close()
  usePanesStore.setState((state) => ({
    panes: {
      ...state.panes,
      left: {
        ...state.panes.left,
        path: 'C:\\root',
        entries: [entry()],
      },
    },
  }))
})

describe('dispatchContextMenuAction', () => {
  it('routes Open With through IPC when selected from the menu', async () => {
    const openWith = vi.fn(() => ({ handled: true }))
    ipc.override('open_with', openWith)

    dispatchContextMenuAction('left', {
      kind: 'open-with',
      path: 'C:\\root\\Report.txt',
    })

    expect(openWith).toHaveBeenCalledWith({ path: 'C:\\root\\Report.txt' })
  })

  it('logs unsupported Open With without falling back to default open', async () => {
    const openWith = vi.fn(() => ({ handled: false, message: 'unsupported' }))
    const openPath = vi.fn(() => undefined)
    const frontendLog = vi.fn(() => undefined)
    ipc.override('open_with', openWith)
    ipc.override('open_path', openPath)
    ipc.override('log_frontend', frontendLog)

    dispatchContextMenuAction('left', {
      kind: 'open-with',
      path: 'C:\\root\\Report.txt',
    })

    await waitFor(() => {
      expect(frontendLog).toHaveBeenCalledWith(
        expect.objectContaining({ level: 'warn', message: 'open_with unavailable' }),
      )
    })
    expect(openPath).not.toHaveBeenCalled()
  })

  it('opens the fallback properties dialog when native properties are unavailable', async () => {
    ipc.override('show_properties', () => ({ handled: false, message: 'unsupported' }))

    dispatchContextMenuAction('left', {
      kind: 'properties',
      items: [
        {
          ...entry(),
        },
      ],
    })
    await waitFor(() => {
      expect(usePropertiesDialogStore.getState().dialog).toMatchObject({
        items: [expect.objectContaining({ path: 'C:\\root\\Report.txt' })],
      })
    })
  })

  it('opens archive confirmation dialogs for compress and extract actions', () => {
    dispatchContextMenuAction('left', {
      kind: 'compress',
      paths: ['C:\\root\\Report.txt'],
      destinationDir: 'C:\\root',
    })
    expect(useActionDialogStore.getState().dialog).toMatchObject({
      kind: 'archiveConfirm',
      operation: 'compress',
      destinationDir: 'C:\\root',
      targets: [{ name: 'Report.txt', path: 'C:\\root\\Report.txt' }],
    })

    dispatchContextMenuAction('left', {
      kind: 'extract',
      paths: ['C:\\root\\Archive.zip'],
      destinationDir: 'C:\\root',
    })
    expect(useActionDialogStore.getState().dialog).toMatchObject({
      kind: 'archiveConfirm',
      operation: 'extract',
      destinationDir: 'C:\\root',
      targets: [{ name: 'Archive.zip', path: 'C:\\root\\Archive.zip', sizeBytes: 0 }],
    })
  })

  it('logs unsupported share and routes native actions through IPC quietly', async () => {
    const frontendLog = vi.fn(() => undefined)
    const invokeNative = vi.fn(() => ({ handled: true }))
    ipc.override('log_frontend', frontendLog)
    ipc.override('invoke_native_menu_action', invokeNative)

    dispatchContextMenuAction('left', {
      kind: 'share',
      paths: ['C:\\root\\Report.txt'],
    })
    dispatchContextMenuAction('left', {
      kind: 'invoke-native',
      token: 'native-token',
    })

    await Promise.resolve()

    expect(invokeNative).toHaveBeenCalledWith({ token: 'native-token' })
    expect(frontendLog).toHaveBeenCalledWith(
      expect.objectContaining({ level: 'info', message: 'share command unavailable' }),
    )
  })
})
