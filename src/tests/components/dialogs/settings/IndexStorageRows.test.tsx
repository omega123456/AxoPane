import { act, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { SettingsModal } from '@/components/dialogs/SettingsModal'
import type { FolderSizeIndexStatus, FolderSizeIndexStorage } from '@/lib/types/ipc'
import { ipc } from '@/tests/ipc-mock'
import { useConfigStore } from '@/stores/config-store'
import { usePanesStore } from '@/stores/panes-store'
import { useSettingsStore } from '@/stores/settings-store'

const CACHE_FOLDER = '/Users/alex/Library/Caches/com.axopane.app'
const BUILD_CACHE = '/Users/alex/Projects/app/build-cache'

const savedIndex: FolderSizeIndexStorage = {
  directory: CACHE_FOLDER,
  sizeBytes: 78_643_200,
  folderCount: 658_631,
  fileCount: 4_193_422,
}

const spillingIndex: FolderSizeIndexStorage = {
  directory: CACHE_FOLDER,
  sizeBytes: 2048,
  folderCount: 1200,
}

const originalOpenTabFromPath = usePanesStore.getState().openTabFromPath

function indexStatus(state: FolderSizeIndexStatus['state']): FolderSizeIndexStatus {
  return { backend: 'macIndex', isAvailable: true, state, fileNamesAvailable: true }
}

function setIndexState(state: FolderSizeIndexStatus['state']) {
  usePanesStore.setState({ folderSizeIndexStatus: indexStatus(state) })
}

async function openSearchIndex() {
  useSettingsStore.getState().open('searchIndex')
  render(<SettingsModal />)
  await screen.findByText('AxoPane 0.1.0')
}

function rebuildButton() {
  return screen.getByRole('button', { name: 'Rebuild' })
}

describe('IndexStorageRows', () => {
  beforeEach(() => {
    ipc.install()
    ipc.override('save_config', (payload) => payload.config)
    ipc.override('folder_size_index_storage', savedIndex)
    useSettingsStore.getState().close()
    useConfigStore.getState().reset()
    usePanesStore.getState().reset()
    setIndexState('upToDate')
  })

  afterEach(() => {
    usePanesStore.setState({ openTabFromPath: originalOpenTabFromPath })
  })

  it('shows where the index is saved, its size and every indexed item', async () => {
    await openSearchIndex()

    expect(await screen.findByText(CACHE_FOLDER)).toBeInTheDocument()
    expect(screen.getByText('75.0 MB · 658,631 folders · 4,193,422 files')).toBeInTheDocument()
    expect(rebuildButton()).toBeEnabled()
  })

  it('explains the missing file count and blocks a rebuild while indexing', async () => {
    ipc.override('folder_size_index_storage', spillingIndex)
    setIndexState('indexing')
    await openSearchIndex()

    expect(
      await screen.findByText('2.0 KB · 1,200 folders · files are counted once indexing finishes'),
    ).toBeInTheDocument()
    expect(rebuildButton()).toBeDisabled()
  })

  it('shows only the size when the index has nothing to count', async () => {
    ipc.override('folder_size_index_storage', { directory: CACHE_FOLDER, sizeBytes: 0 })
    setIndexState('notReady')
    await openSearchIndex()

    expect(await screen.findByText('0 B')).toBeInTheDocument()
    expect(rebuildButton()).toBeDisabled()
  })

  it('closes settings and opens the index folder in a new tab of the active pane', async () => {
    const user = userEvent.setup()
    const openTab = vi.fn(() => Promise.resolve())
    usePanesStore.setState({ openTabFromPath: openTab, activePaneId: 'right' })
    await openSearchIndex()

    await user.click(await screen.findByRole('button', { name: 'Open in new tab' }))

    expect(openTab).toHaveBeenCalledWith('right', CACHE_FOLDER)
    expect(useSettingsStore.getState().isOpen).toBe(false)
  })

  it('asks Rust to rebuild the index', async () => {
    const user = userEvent.setup()
    const rebuild = vi.fn()
    ipc.override('rebuild_folder_size_index', rebuild)
    await openSearchIndex()

    await user.click(rebuildButton())

    await waitFor(() => expect(rebuild).toHaveBeenCalledTimes(1))
  })

  it('reloads the storage when the index state or the exclusions change', async () => {
    const storage = vi.fn(() => spillingIndex)
    ipc.override('folder_size_index_storage', storage)
    setIndexState('indexing')
    await openSearchIndex()
    await screen.findByText(/1,200 folders/)

    storage.mockReturnValue(savedIndex)
    act(() => setIndexState('upToDate'))
    expect(await screen.findByText(/4,193,422 files/)).toBeInTheDocument()

    act(() => useConfigStore.setState({ indexExclusions: [BUILD_CACHE] }))
    await waitFor(() => expect(storage).toHaveBeenCalledTimes(3))
  })

  it('keeps the rebuild row but hides the files row when the storage is unavailable', async () => {
    ipc.override('folder_size_index_storage', () => Promise.reject(new Error('unsupported')))
    await openSearchIndex()

    expect(rebuildButton()).toBeEnabled()
    await waitFor(() => expect(screen.queryByText('Index files')).not.toBeInTheDocument())
  })
})
