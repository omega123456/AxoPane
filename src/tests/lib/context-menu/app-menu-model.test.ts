import { beforeEach, describe, expect, it } from 'vitest'
import { buildAppContextMenuContent } from '@/lib/context-menu/app-menu-model'
import { TRASH_PATH } from '@/lib/trash'
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

const zipEntry = {
  ...fileEntry,
  id: 'archive',
  name: 'Archive.zip',
  path: 'C:\\Users\\Omega\\Archive.zip',
  typeLabel: 'ZIP archive',
}

describe('buildAppContextMenuContent', () => {
  beforeEach(() => {
    useClipboardStore.getState().clearClipboard()
    useSelectionStore.getState().reset()
    useKeymapStore.getState().reset()
    useTabsStore.getState().reset()
    usePanesStore.getState().reset()
    usePanesStore.setState({
      everythingStatus: { status: 'unavailable', isAvailable: false },
      volumes: [
        {
          mountRoot: 'C:\\',
          label: 'System',
          totalBytes: 1,
          freeBytes: 1,
          isNetwork: false,
          isRemovable: false,
        },
        {
          mountRoot: '\\\\server\\',
          label: 'Share',
          totalBytes: 1,
          freeBytes: 1,
          isNetwork: true,
          isRemovable: false,
        },
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

  it('builds folder content with the quick strip and Open first in the primary group', () => {
    const content = buildAppContextMenuContent(
      'left',
      { kind: 'folder', entry: folderEntry },
      'windows',
    )

    expect(content.topStrip.map((item) => item.label)).toEqual(['Cut', 'Copy', 'Rename', 'Delete'])
    expect(content.sections.map((section) => section.id)).toEqual([
      'primary',
      'secondary',
      'footer',
    ])
    expect(content.sections[0]?.rows.map((row) => row.label)).toEqual([
      'Open',
      'Open in new tab',
      'Open in other pane',
      'Open with',
    ])
    expect(content.sections[1]?.rows.map((row) => row.label)).toContain('Copy as path')
    const appRows = content.sections
      .flatMap((section) => section.rows)
      .filter((row) => row.owner === 'app')
    expect(appRows.every((row) => row.icon?.kind === 'app')).toBe(true)
    expect(content.sections[0]?.rows[0]?.strong).toBe(true)
    expect(content.sections[1]?.rows.find((row) => row.label === 'Calculate size')?.disabled).toBe(
      false,
    )
    expect(
      content.sections[1]?.rows.find((row) => row.label === 'Delete permanently'),
    ).toMatchObject({ danger: true })
    expect(content.sections[2]?.rows.map((row) => row.label)).toEqual(['Properties'])
  })

  it('keeps unsupported file actions disabled and hides folder-size rows when Everything is available', () => {
    usePanesStore.setState({ everythingStatus: { status: 'available', isAvailable: true } })

    const folderContent = buildAppContextMenuContent(
      'left',
      { kind: 'folder', entry: folderEntry },
      'windows',
    )
    const fileContent = buildAppContextMenuContent(
      'left',
      { kind: 'file', entry: fileEntry },
      'macos',
    )
    const zipContent = buildAppContextMenuContent(
      'left',
      { kind: 'file', entry: zipEntry },
      'windows',
    )

    expect(
      folderContent.sections[1]?.rows.find((row) => row.label === 'Calculate size')?.hidden,
    ).toBe(true)
    expect(fileContent.sections[1]?.rows.find((row) => row.label === 'Extract')?.disabled).toBe(
      true,
    )
    expect(zipContent.sections[1]?.rows.find((row) => row.label === 'Extract')?.disabled).toBe(
      false,
    )
    expect(
      fileContent.sections[0]?.rows.find((row) => row.label === 'Open in new tab')?.disabled,
    ).toBe(true)
    expect(
      fileContent.sections[0]?.rows.find((row) => row.label === 'Open in new tab')?.shortcut,
    ).toBe('⌘Enter')
    expect(
      fileContent.sections[0]?.rows.find((row) => row.label === 'Open in other pane')?.shortcut,
    ).toBe('⌘⇧Enter')
  })

  it('uses a reduced strip for multi-select and keeps tree menus app-owned with disabled network size calculation', () => {
    useSelectionStore
      .getState()
      .setSelection('left', [folderEntry.id, fileEntry.id], folderEntry.id, fileEntry.id)

    const multiContent = buildAppContextMenuContent('left', { kind: 'multi' }, 'windows')
    const treeContent = buildAppContextMenuContent(
      'left',
      { kind: 'tree', path: '\\\\server\\Share' },
      'windows',
    )

    expect(multiContent.topStrip.map((item) => item.label)).toEqual(['Cut', 'Copy', 'Delete'])
    expect(multiContent.sections[0]?.rows.map((row) => row.label)).toContain('Copy as path')
    expect(
      multiContent.sections[0]?.rows.find((row) => row.label === 'Delete permanently'),
    ).toMatchObject({ danger: true, disabled: false })
    expect(multiContent.sections[1]?.rows.map((row) => row.label)).toEqual(['Properties'])
    expect(treeContent.topStrip).toEqual([])
    expect(treeContent.sections[0]?.rows[0]?.label).toBe('Open')
    expect(treeContent.sections[1]?.rows.map((row) => row.label)).toContain('Copy as path')
    expect(
      treeContent.sections[1]?.rows.find((row) => row.label === 'Calculate size')?.disabled,
    ).toBe(true)
    expect(treeContent.sections[2]?.rows.map((row) => row.label)).toEqual(['Properties'])
  })

  it('only enables Extract for all-ZIP multi-selections', () => {
    usePanesStore.setState((state) => ({
      panes: {
        ...state.panes,
        left: {
          ...state.panes.left,
          entries: [
            zipEntry,
            {
              ...zipEntry,
              id: 'archive-2',
              path: 'C:\\Users\\Omega\\Other.ZIP',
              name: 'Other.ZIP',
            },
            fileEntry,
          ],
        },
      },
    }))

    useSelectionStore
      .getState()
      .setSelection('left', [zipEntry.id, 'archive-2'], zipEntry.id, 'archive-2')
    const zipOnlyContent = buildAppContextMenuContent('left', { kind: 'multi' }, 'windows')

    useSelectionStore
      .getState()
      .setSelection('left', [zipEntry.id, fileEntry.id], zipEntry.id, fileEntry.id)
    const mixedContent = buildAppContextMenuContent('left', { kind: 'multi' }, 'windows')

    expect(zipOnlyContent.sections[0]?.rows.find((row) => row.label === 'Extract')?.disabled).toBe(
      false,
    )
    expect(mixedContent.sections[0]?.rows.find((row) => row.label === 'Extract')?.disabled).toBe(
      true,
    )
  })

  it('adds a properties footer to empty background menus', () => {
    const content = buildAppContextMenuContent('left', { kind: 'empty' }, 'windows')

    expect(content.sections.at(-1)?.rows.map((row) => row.label)).toEqual(['Properties'])
  })

  describe('trash pane', () => {
    const trashEntryWithOriginal = {
      ...fileEntry,
      id: 'report.txt',
      trashId: 'report.txt',
      originalPath: 'C:\\Users\\Omega\\Report.txt',
    }
    const trashEntryOrphaned = {
      ...fileEntry,
      id: 'orphan.txt',
      name: 'orphan.txt',
      trashId: 'orphan.txt',
      originalPath: undefined,
    }

    beforeEach(() => {
      usePanesStore.setState((state) => ({
        panes: {
          ...state.panes,
          left: {
            ...state.panes.left,
            path: TRASH_PATH,
            entries: [trashEntryWithOriginal, trashEntryOrphaned],
          },
        },
      }))
    })

    it('offers restore and delete-permanently for a single trash entry, disabling restore without an original path', () => {
      const restorable = buildAppContextMenuContent(
        'left',
        { kind: 'file', entry: trashEntryWithOriginal },
        'windows',
      )
      expect(restorable.sections[0]?.rows.map((row) => row.label)).toEqual([
        'Restore',
        'Delete permanently',
      ])
      expect(restorable.sections[0]?.rows[0]).toMatchObject({ disabled: false })
      expect(restorable.sections[0]?.rows[1]).toMatchObject({ danger: true })

      const orphaned = buildAppContextMenuContent(
        'left',
        { kind: 'file', entry: trashEntryOrphaned },
        'windows',
      )
      expect(orphaned.sections[0]?.rows[0]).toMatchObject({ disabled: true })
    })

    it('disables multi-select restore when any selected entry lacks an original path', () => {
      useSelectionStore
        .getState()
        .setSelection(
          'left',
          [trashEntryWithOriginal.id, trashEntryOrphaned.id],
          trashEntryWithOriginal.id,
          trashEntryOrphaned.id,
        )

      const content = buildAppContextMenuContent('left', { kind: 'multi' }, 'windows')
      expect(content.sections[0]?.rows.map((row) => row.label)).toEqual([
        'Restore',
        'Delete permanently',
      ])
      expect(content.sections[0]?.rows[0]).toMatchObject({ disabled: true })
    })

    it('offers a single Empty trash action for the trash tree row and the pane background', () => {
      const treeContent = buildAppContextMenuContent('left', { kind: 'tree', path: TRASH_PATH }, 'windows')
      expect(treeContent.sections[0]?.rows.map((row) => row.label)).toEqual(['Empty Trash'])

      const emptyContent = buildAppContextMenuContent('left', { kind: 'empty' }, 'windows')
      expect(emptyContent.sections[0]?.rows.map((row) => row.label)).toEqual(['Empty Trash'])
    })
  })
})
