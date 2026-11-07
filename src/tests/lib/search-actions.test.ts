import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ipc } from '@/tests/ipc-mock'
import { mergeKeymap } from '@/lib/keymap'
import {
  activateSearchHit,
  completeSearchQuery,
  searchActivationForEvent,
} from '@/lib/search-actions'
import type { SearchHit } from '@/lib/types/ipc'
import { usePanesStore } from '@/stores/panes-store'

const folder: SearchHit = {
  path: 'C:\\Users\\alex\\Invoices',
  name: 'Invoices',
  parentPath: 'C:\\Users\\alex',
  isDir: true,
  modifiedAt: null,
  nameRanges: [],
  parentRanges: [],
}

const file: SearchHit = {
  ...folder,
  path: 'C:\\Users\\alex\\Invoices\\invoice-0412.pdf',
  name: 'invoice-0412.pdf',
  parentPath: 'C:\\Users\\alex\\Invoices',
  isDir: false,
}

type PanesState = ReturnType<typeof usePanesStore.getState>

let navigatePane: ReturnType<typeof vi.fn<PanesState['navigatePane']>>
let openTabFromPath: ReturnType<typeof vi.fn<PanesState['openTabFromPath']>>
const original = usePanesStore.getState()

beforeEach(() => {
  navigatePane = vi.fn<PanesState['navigatePane']>(() => Promise.resolve())
  openTabFromPath = vi.fn<PanesState['openTabFromPath']>(() => Promise.resolve())
  usePanesStore.setState({ activePaneId: 'left', navigatePane, openTabFromPath })
})

afterEach(() => {
  usePanesStore.setState({
    navigatePane: original.navigatePane,
    openTabFromPath: original.openTabFromPath,
  })
})

describe('activateSearchHit', () => {
  it('opens folders and reveals files in a new tab of the active pane', async () => {
    await activateSearchHit(folder, 'default')
    expect(openTabFromPath).toHaveBeenCalledWith('left', folder.path, { revealPath: undefined })
    await activateSearchHit(folder, 'open')
    expect(openTabFromPath).toHaveBeenCalledTimes(2)
    await activateSearchHit(file, 'default')
    expect(openTabFromPath).toHaveBeenLastCalledWith('left', folder.path, {
      revealPath: file.path,
    })
    expect(navigatePane).not.toHaveBeenCalled()
  })

  it('targets a new tab or the other pane', async () => {
    await activateSearchHit(file, 'newTab')
    expect(openTabFromPath).toHaveBeenCalledWith('left', folder.path, { revealPath: file.path })
    await activateSearchHit(folder, 'otherPane')
    expect(openTabFromPath).toHaveBeenLastCalledWith('right', folder.path, {
      revealPath: undefined,
    })
    usePanesStore.setState({ activePaneId: 'right' })
    await activateSearchHit(folder, 'otherPane')
    expect(openTabFromPath).toHaveBeenLastCalledWith('left', folder.path, {
      revealPath: undefined,
    })
  })

  it('launches files with their default app and survives a failure', async () => {
    const opened: string[] = []
    ipc.override('open_path', (request) => {
      opened.push(request.path)
    })
    await activateSearchHit(file, 'open')
    expect(opened).toEqual([file.path])

    ipc.override('open_path', () => Promise.reject(new Error('no app')))
    await expect(activateSearchHit(file, 'open')).resolves.toBeUndefined()
    expect(navigatePane).not.toHaveBeenCalled()
  })

  it('falls back to the path itself for a root-level file', async () => {
    await activateSearchHit({ ...file, path: 'C:\\' }, 'default')
    expect(openTabFromPath).toHaveBeenCalledWith('left', 'C:\\', { revealPath: 'C:\\' })
  })
})

describe('searchActivationForEvent', () => {
  const keymap = mergeKeymap({ openInNewTab: ['Alt+Enter'] })
  const key = (init: KeyboardEventInit) => new KeyboardEvent('keydown', { key: 'Enter', ...init })

  it('maps Enter variants and the remappable tab and pane bindings', () => {
    expect(searchActivationForEvent(key({}), keymap)).toBe('default')
    expect(searchActivationForEvent(key({ shiftKey: true }), keymap)).toBe('open')
    expect(searchActivationForEvent(key({ altKey: true }), keymap)).toBe('newTab')
    expect(searchActivationForEvent(key({ ctrlKey: true, shiftKey: true }), keymap)).toBe(
      'otherPane',
    )
    expect(searchActivationForEvent(key({ key: 'a' }), keymap)).toBeNull()
  })
})

describe('completeSearchQuery', () => {
  it('replaces the last segment with the folder name and a separator', () => {
    expect(completeSearchQuery('alex', folder)).toBe('Invoices/')
    expect(completeSearchQuery('alex/inv', folder)).toBe('alex/Invoices/')
    expect(completeSearchQuery('C:\\Users\\al', folder)).toBe('C:\\Users\\Invoices\\')
    expect(completeSearchQuery('~/', folder)).toBe('~/Invoices/')
  })
})
