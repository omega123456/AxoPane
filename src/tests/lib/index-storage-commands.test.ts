import { beforeEach, describe, expect, it, vi } from 'vitest'
import { loadIndexStorage, rebuildIndex } from '@/lib/index-storage-commands'
import { ipc } from '@/tests/ipc-mock'

const STORAGE = {
  directory: '/Users/alex/Library/Caches/com.axopane.app',
  sizeBytes: 2048,
  folderCount: 3,
  fileCount: 7,
}

function rejectUnsupported() {
  return Promise.reject(new Error('unsupported'))
}

describe('index-storage-commands', () => {
  beforeEach(() => {
    ipc.install()
  })

  it('loads what Rust reports about the saved index', async () => {
    ipc.override('folder_size_index_storage', STORAGE)

    await expect(loadIndexStorage()).resolves.toEqual(STORAGE)
  })

  it('asks Rust to rebuild the index', async () => {
    const rebuild = vi.fn()
    ipc.override('rebuild_folder_size_index', rebuild)

    await rebuildIndex()

    expect(rebuild).toHaveBeenCalledTimes(1)
  })

  it('degrades to no storage and no rebuild when the commands are unsupported', async () => {
    ipc.override('folder_size_index_storage', rejectUnsupported)
    ipc.override('rebuild_folder_size_index', rejectUnsupported)

    await expect(loadIndexStorage()).resolves.toBeNull()
    await expect(rebuildIndex()).resolves.toBeUndefined()
  })
})
