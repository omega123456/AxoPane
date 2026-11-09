import { beforeEach, describe, expect, it, vi } from 'vitest'
import {
  excludeFolders,
  includeFolder,
  pickFoldersToExclude,
  retryIndexStart,
} from '@/lib/index-exclusions-commands'
import { ipc } from '@/tests/ipc-mock'
import { useConfigStore } from '@/stores/config-store'

const BUILD_CACHE = '/Users/alex/Projects/app/build-cache'

function rejectUnsupported() {
  return Promise.reject(new Error('unsupported'))
}

describe('index-exclusions-commands', () => {
  beforeEach(() => {
    ipc.install()
    useConfigStore.getState().reset()
  })

  it('adds exclusions, stores the list Rust returns and hands back the outcomes', async () => {
    const add = vi.fn(() => ({
      exclusions: [BUILD_CACHE],
      outcomes: [{ path: BUILD_CACHE, outcome: 'added' as const }],
    }))
    ipc.override('add_index_exclusion', add)

    const outcomes = await excludeFolders([BUILD_CACHE])

    expect(add).toHaveBeenCalledWith({ paths: [BUILD_CACHE] })
    expect(outcomes).toEqual([{ path: BUILD_CACHE, outcome: 'added' }])
    expect(useConfigStore.getState().indexExclusions).toEqual([BUILD_CACHE])
  })

  it('removes an exclusion and stores the remaining list', async () => {
    useConfigStore.setState({ indexExclusions: [BUILD_CACHE] })
    const remove = vi.fn(() => ({ exclusions: [] }))
    ipc.override('remove_index_exclusion', remove)

    await expect(includeFolder(BUILD_CACHE)).resolves.toBe(true)

    expect(remove).toHaveBeenCalledWith({ path: BUILD_CACHE })
    expect(useConfigStore.getState().indexExclusions).toEqual([])
  })

  it('returns the picked folders and retries the index', async () => {
    const retry = vi.fn()
    ipc.override('pick_index_exclusions', { paths: [BUILD_CACHE] })
    ipc.override('retry_folder_size_index', retry)

    await expect(pickFoldersToExclude()).resolves.toEqual([BUILD_CACHE])
    await retryIndexStart()

    expect(retry).toHaveBeenCalledTimes(1)
  })

  it('degrades to no change when the commands are unsupported', async () => {
    useConfigStore.setState({ indexExclusions: [BUILD_CACHE] })
    ipc.override('add_index_exclusion', rejectUnsupported)
    ipc.override('remove_index_exclusion', rejectUnsupported)
    ipc.override('pick_index_exclusions', rejectUnsupported)
    ipc.override('retry_folder_size_index', rejectUnsupported)

    await expect(excludeFolders([BUILD_CACHE])).resolves.toEqual([])
    await expect(includeFolder(BUILD_CACHE)).resolves.toBe(false)
    await expect(pickFoldersToExclude()).resolves.toEqual([])
    await expect(retryIndexStart()).resolves.toBeUndefined()
    expect(useConfigStore.getState().indexExclusions).toEqual([BUILD_CACHE])
  })
})
