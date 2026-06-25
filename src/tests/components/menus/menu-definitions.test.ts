import { beforeEach, describe, expect, it } from 'vitest'
import { buildContextMenuItems, resolveMenuTarget } from '@/components/menus/menu-definitions'
import { useClipboardStore } from '@/stores/clipboard-store'
import { useKeymapStore } from '@/stores/keymap-store'
import { usePanesStore } from '@/stores/panes-store'
import { useSelectionStore } from '@/stores/selection-store'
import { useTabsStore } from '@/stores/tabs-store'

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
    useClipboardStore.getState().clearClipboard()
    useSelectionStore.getState().reset()
    useKeymapStore.getState().reset()
    useTabsStore.getState().reset()
    usePanesStore.getState().reset()
    usePanesStore.setState({
      everythingStatus: { status: 'unavailable', isAvailable: false },
      volumes: [
        { mountRoot: 'C:\\', label: 'System', totalBytes: 1, freeBytes: 1, isNetwork: false },
        { mountRoot: '\\\\server\\', label: 'Share', totalBytes: 1, freeBytes: 1, isNetwork: true },
      ],
      panes: {
        left: {
          ...usePanesStore.getState().panes.left,
          path: 'C:\\Users\\Omega',
          entries: [folderEntry, fileEntry],
          focusedEntryId: folderEntry.id,
        },
        right: usePanesStore.getState().panes.right,
      },
    })
  })

  it('builds folder menus with enabled calculate size for manual capability', () => {
    const items = buildContextMenuItems('left', { kind: 'folder', entry: folderEntry }, 'windows')
    const calculate = items.find((item) => item.id === 'calculateSize')
    expect(calculate?.disabled).toBe(false)
    expect(calculate?.shortcut).toBe('Space')
  })

  it('hides calculate size when Everything is available and disables paste without clipboard data', () => {
    usePanesStore.setState({ everythingStatus: { status: 'available', isAvailable: true } })
    const items = buildContextMenuItems('left', { kind: 'folder', entry: folderEntry }, 'windows')
    const calculate = items.find((item) => item.id === 'calculateSize')
    const paste = items.find((item) => item.id === 'paste')
    expect(calculate?.hidden).toBe(true)
    expect(paste?.disabled).toBe(true)
  })

  it('resolves multi-select targets and keeps unsupported file actions disabled instead of hidden', () => {
    useSelectionStore.getState().setSelection('left', [folderEntry.id, fileEntry.id], folderEntry.id, fileEntry.id)
    const target = resolveMenuTarget('left', folderEntry)
    expect(target.kind).toBe('multi')

    const fileItems = buildContextMenuItems('left', { kind: 'file', entry: fileEntry }, 'macos')
    expect(fileItems.find((item) => item.id === 'openInNewTab')?.disabled).toBe(true)
    expect(fileItems.find((item) => item.id === 'openInNewTab')?.shortcut).toBe('⌘Enter')
  })
})
