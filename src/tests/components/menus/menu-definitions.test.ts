import { beforeEach, describe, expect, it } from 'vitest'
import {
  buildContextMenuContent,
  describeMenuTarget,
  resolveMenuTarget,
} from '@/components/menus/menu-definitions'
import { usePanesStore } from '@/stores/panes-store'
import { useSelectionStore } from '@/stores/selection-store'

const folderEntry = {
  id: 'docs',
  name: 'Documents',
  path: 'C:\\Users\\Omega\\Documents',
  isDir: true,
  sizeBytes: null,
  itemCount: 3,
  typeLabel: 'Folder',
  modifiedAt: null,
  createdAt: null,
  attributes: [],
  isHidden: false,
  isSystem: false,
}

const fileEntry = {
  ...folderEntry,
  id: 'report',
  name: 'Report.txt',
  path: 'C:\\Users\\Omega\\Report.txt',
  isDir: false,
  typeLabel: 'TXT file',
}

describe('menu definitions', () => {
  beforeEach(() => {
    usePanesStore.getState().reset()
    useSelectionStore.getState().reset()
  })

  it('resolves multi-select targets when the right-clicked entry is inside the current selection', () => {
    useSelectionStore
      .getState()
      .setSelection('left', [folderEntry.id, fileEntry.id], folderEntry.id, fileEntry.id)
    expect(resolveMenuTarget('left', folderEntry)).toEqual({ kind: 'multi' })
    expect(resolveMenuTarget('left')).toEqual({ kind: 'multi' })
  })

  it('describes header metadata for file, folder, and multi-select targets', () => {
    expect(describeMenuTarget({ kind: 'file', entry: fileEntry })).toEqual({
      title: 'Report.txt',
      chip: 'TXT',
    })
    expect(describeMenuTarget({ kind: 'folder', entry: folderEntry })).toEqual({
      title: 'Documents',
      chip: 'DIR',
    })
    expect(describeMenuTarget({ kind: 'multi' })).toEqual({
      title: 'Multiple items',
    })
  })

  it('builds native enrichment requests for file, mixed multi-select, tree, and tab targets', () => {
    usePanesStore.setState((state) => ({
      panes: {
        ...state.panes,
        left: {
          ...state.panes.left,
          path: 'C:\\Users\\Omega',
          entries: [folderEntry, fileEntry],
        },
      },
      volumes: [
        {
          mountRoot: 'C:\\',
          label: 'Windows',
          totalBytes: 1,
          freeBytes: 1,
          isNetwork: false,
          isRemovable: false,
        },
      ],
    }))
    useSelectionStore
      .getState()
      .setSelection('left', [folderEntry.id, fileEntry.id], folderEntry.id, fileEntry.id)

    expect(
      buildContextMenuContent('left', { kind: 'file', entry: fileEntry }, 'windows').nativeRequest,
    ).toEqual({
      targetKind: 'file',
      targetPath: fileEntry.path,
      folderPath: 'C:\\Users\\Omega',
      selectedPaths: [fileEntry.path],
    })

    expect(buildContextMenuContent('left', { kind: 'multi' }, 'windows').nativeRequest).toEqual({
      targetKind: 'mixed',
      targetPath: null,
      folderPath: 'C:\\Users\\Omega',
      selectedPaths: [folderEntry.path, fileEntry.path],
    })

    expect(
      buildContextMenuContent('left', { kind: 'tree', path: 'C:\\' }, 'windows').nativeRequest,
    ).toEqual({
      targetKind: 'driveRoot',
      targetPath: 'C:\\',
      folderPath: 'C:\\',
      selectedPaths: ['C:\\'],
    })

    expect(
      buildContextMenuContent('left', { kind: 'tab', tabId: 'tab-1' }, 'windows').nativeRequest,
    ).toBeNull()
  })
})
