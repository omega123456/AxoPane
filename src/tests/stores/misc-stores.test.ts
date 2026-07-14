import { beforeEach, vi } from 'vitest'
import { commandContextAction, noopContextAction } from '@/lib/context-menu/context-menu-actions'
import { useConfigStore } from '@/stores/config-store'
import { useContextMenuStore } from '@/stores/context-menu-store'
import { useKeymapStore } from '@/stores/keymap-store'
import { useLayoutStore } from '@/stores/layout-store'
import { clearSelectionForPane, useSelectionStore } from '@/stores/selection-store'
import { initializeTheme, useThemeStore } from '@/stores/theme-store'

beforeEach(() => {
  useContextMenuStore.getState().closeMenu()
  useConfigStore.getState().reset()
  useKeymapStore.getState().reset()
  useLayoutStore.getState().reset()
  useSelectionStore.getState().reset()
  document.documentElement.classList.remove('dark')
})

describe('layout-store', () => {
  it('toggles details visibility and resets', () => {
    useLayoutStore.getState().setDetailsVisible(false)
    expect(useLayoutStore.getState().detailsVisible).toBe(false)
    useLayoutStore.getState().setDetailsVisible(true)
    expect(useLayoutStore.getState().detailsVisible).toBe(true)
    useLayoutStore.getState().reset()
    expect(useLayoutStore.getState().detailsVisible).toBe(false)
  })

  it('preserves a user-defined column order through setColumns and hydrate', () => {
    const reordered = [
      { key: 'modified' as const, visible: true },
      { key: 'name' as const, visible: true },
      { key: 'size' as const, visible: false },
    ]

    useLayoutStore.getState().setColumns(reordered)
    const afterSet = useLayoutStore.getState().columns
    expect(afterSet.slice(0, 3).map((column) => column.key)).toEqual(['modified', 'name', 'size'])
    // Missing schema columns are appended, not dropped.
    expect(afterSet.map((column) => column.key).sort()).toEqual(
      ['created', 'items', 'modified', 'name', 'size', 'type'].sort(),
    )
    expect(afterSet.find((column) => column.key === 'size')?.visible).toBe(false)

    useLayoutStore.getState().reset()
    useLayoutStore.getState().hydrate(useLayoutStore.getState(), reordered)
    expect(
      useLayoutStore
        .getState()
        .columns.slice(0, 3)
        .map((column) => column.key),
    ).toEqual(['modified', 'name', 'size'])
  })

  it('moves a column to a new position', () => {
    useLayoutStore.getState().reset()
    useLayoutStore.getState().moveColumn('name', 'type')
    const keys = useLayoutStore.getState().columns.map((column) => column.key)
    expect(keys.indexOf('name')).toBeGreaterThan(keys.indexOf('items'))
  })

  it('ignores duplicate and unknown saved columns while preserving defaults for missing columns', () => {
    useLayoutStore.getState().setColumns([
      { key: 'size', visible: false },
      { key: 'size', visible: true },
      { key: 'created', visible: true },
      { key: 'not-a-column' as never, visible: true },
    ])

    expect(useLayoutStore.getState().columns).toEqual([
      { key: 'size', visible: false },
      { key: 'created', visible: true },
      { key: 'name', visible: true },
      { key: 'items', visible: true },
      { key: 'type', visible: true },
      { key: 'modified', visible: true },
    ])
  })

  it('keeps column order unchanged when asked to move a missing column or onto itself', () => {
    const before = useLayoutStore.getState().columns

    useLayoutStore.getState().moveColumn('name', 'name')
    expect(useLayoutStore.getState().columns).toBe(before)

    useLayoutStore.getState().moveColumn('not-a-column' as never, 'type')
    expect(useLayoutStore.getState().columns).toBe(before)
  })

  it('updates scalar layout preferences and toggles individual columns', () => {
    const setProperty = vi.spyOn(document.documentElement.style, 'setProperty')
    // Provide the app root so applyZoom exercises its height-compensation path
    // (the zoomed root is counter-scaled so 100vh content never scrolls the page).
    const appRoot = document.createElement('div')
    appRoot.id = 'root'
    document.body.appendChild(appRoot)
    useLayoutStore.getState().setTreeWidthPx(320)
    useLayoutStore.getState().setPaneSplit(0.35)
    useLayoutStore.getState().setDefaultPaneMode('single')
    useLayoutStore.getState().setDefaultViewMode('thumbnails')
    useLayoutStore.getState().setRestoreSession(false)
    useLayoutStore.getState().setZoom('125')
    useLayoutStore.getState().setColumnWidth('type', 188)
    useLayoutStore.getState().toggleColumn('created')

    expect(useLayoutStore.getState()).toMatchObject({
      treeWidthPx: 320,
      paneSplit: 0.35,
      columnWidths: expect.objectContaining({ type: 188 }),
      defaultPaneMode: 'single',
      defaultViewMode: 'thumbnails',
      restoreSession: false,
      zoom: '125',
    })
    expect(setProperty).toHaveBeenCalledWith('zoom', '1.25')
    expect(document.documentElement.style.height).toBe('80vh')
    expect(appRoot.style.height).toBe('100%')
    appRoot.remove()
    setProperty.mockRestore()
    expect(useLayoutStore.getState().columns.find((column) => column.key === 'created')).toEqual({
      key: 'created',
      visible: true,
    })
  })

  it('defaults, hydrates, and validates the default tab view', () => {
    expect(useLayoutStore.getState().defaultViewMode).toBe('details')
    useLayoutStore.getState().hydrate(
      { ...useLayoutStore.getState(), defaultViewMode: 'icons' },
      useLayoutStore.getState().columns,
    )
    expect(useLayoutStore.getState().defaultViewMode).toBe('icons')

    useLayoutStore.getState().hydrate(
      { ...useLayoutStore.getState(), defaultViewMode: 'unknown' as never },
      useLayoutStore.getState().columns,
    )
    expect(useLayoutStore.getState().defaultViewMode).toBe('details')
  })
})

describe('context-menu-store', () => {
  it('opens with hidden items removed and focuses the first enabled item', () => {
    useContextMenuStore.getState().openMenu({
      paneId: 'left',
      x: 10,
      y: 20,
      title: 'Entry',
      topStrip: [
        {
          id: 'hidden',
          label: 'Hidden',
          owner: 'app',
          icon: { kind: 'app', name: 'copy' },
          hidden: true,
          action: noopContextAction('hidden'),
        },
      ],
      sections: [
        {
          id: 'primary',
          rows: [
            {
              id: 'disabled',
              kind: 'action',
              label: 'Disabled',
              owner: 'app',
              disabled: true,
              action: noopContextAction('disabled'),
            },
            {
              id: 'open',
              kind: 'action',
              label: 'Open',
              owner: 'app',
              action: noopContextAction('open'),
            },
          ],
        },
      ],
    })

    const state = useContextMenuStore.getState()
    expect(state.menu?.topStrip).toEqual([])
    expect(state.menu?.sections[0]?.rows.map((item) => item.id)).toEqual(['disabled', 'open'])
    expect(state.activeItemId).toBe('open')
  })

  it('wraps active movement across enabled items and skips disabled items', () => {
    useContextMenuStore.getState().openMenu({
      paneId: 'left',
      x: 0,
      y: 0,
      title: 'Entry',
      topStrip: [
        {
          id: 'copy',
          label: 'Copy',
          owner: 'app',
          icon: { kind: 'app', name: 'copy' },
          action: noopContextAction('copy'),
        },
      ],
      sections: [
        {
          id: 'primary',
          rows: [
            {
              id: 'disabled',
              kind: 'action',
              label: 'Disabled',
              owner: 'app',
              disabled: true,
              action: noopContextAction('disabled'),
            },
            {
              id: 'rename',
              kind: 'action',
              label: 'Rename',
              owner: 'app',
              action: noopContextAction('rename'),
            },
          ],
        },
      ],
    })

    useContextMenuStore.getState().moveActive(1)
    expect(useContextMenuStore.getState().activeItemId).toBe('rename')
    useContextMenuStore.getState().moveActive(1)
    expect(useContextMenuStore.getState().activeItemId).toBe('copy')
    useContextMenuStore.getState().moveActive(-1)
    expect(useContextMenuStore.getState().activeItemId).toBe('rename')
  })

  it('does not move or activate when no enabled menu item is available', () => {
    useContextMenuStore.getState().openMenu({
      paneId: 'right',
      x: 0,
      y: 0,
      title: 'Entry',
      topStrip: [],
      sections: [
        {
          id: 'primary',
          rows: [
            {
              id: 'disabled',
              kind: 'action',
              label: 'Disabled',
              owner: 'app',
              disabled: true,
              action: noopContextAction('disabled'),
            },
          ],
        },
      ],
    })

    useContextMenuStore.getState().moveActive(1)
    useContextMenuStore.getState().activateCurrent()

    expect(useContextMenuStore.getState().menu?.sections[0]?.rows).toHaveLength(1)
    expect(useContextMenuStore.getState().activeItemId).toBeNull()
  })

  it('opens and closes one submenu branch and dispatches the current command item', () => {
    useContextMenuStore.getState().openMenu({
      paneId: 'left',
      x: 0,
      y: 0,
      title: 'Entry',
      topStrip: [],
      sections: [
        {
          id: 'primary',
          rows: [
            {
              id: 'tools',
              kind: 'submenu',
              label: 'Tools',
              owner: 'app',
              children: {
                id: 'tools-panel',
                rows: [
                  {
                    id: 'share-child',
                    label: 'Share child',
                    owner: 'app',
                    action: noopContextAction('share'),
                  },
                ],
              },
            },
            {
              id: 'settings',
              kind: 'action',
              label: 'Settings',
              owner: 'app',
              action: commandContextAction('showSettings'),
            },
          ],
        },
      ],
    })

    useContextMenuStore.getState().activateCurrent()
    expect(useContextMenuStore.getState().openSubmenuId).toBe('tools')
    expect(useContextMenuStore.getState().activeItemId).toBe('share-child')
    expect(useContextMenuStore.getState().activeSubmenuItemId).toBe('share-child')

    useContextMenuStore.getState().closeSubmenu()
    expect(useContextMenuStore.getState().openSubmenuId).toBeNull()
    expect(useContextMenuStore.getState().activeItemId).toBe('tools')
    expect(useContextMenuStore.getState().activeSubmenuItemId).toBeNull()

    useContextMenuStore.getState().hoverItem('settings')
    useContextMenuStore.getState().activateCurrent()

    expect(useContextMenuStore.getState().menu).toBeNull()
  })
})

describe('keymap-store', () => {
  it('hydrates partial bindings, updates one command, and resets it to the default', () => {
    useKeymapStore.getState().hydrate({ rename: ['Ctrl+R'] })
    expect(useKeymapStore.getState().bindings.rename).toEqual(['Ctrl+R'])
    expect(useKeymapStore.getState().bindings.open).toContain('Enter')

    useKeymapStore.getState().setBinding('delete', ['Ctrl+D'])
    expect(useKeymapStore.getState().bindings.delete).toEqual(['Ctrl+D'])

    useKeymapStore.getState().resetBinding('delete')
    expect(useKeymapStore.getState().bindings.delete).toContain('Delete')
  })
})

describe('selection-store', () => {
  it('sets and clears a pane selection', () => {
    useSelectionStore.getState().setSelection('left', ['a', 'b'], 'a', 'b')
    expect(useSelectionStore.getState().selections.left.selectedIds).toEqual(['a', 'b'])
    clearSelectionForPane('left')
    expect(useSelectionStore.getState().selections.left.selectedIds).toEqual([])
  })
})

describe('theme-store', () => {
  it('applies, toggles, and initializes the theme', () => {
    useThemeStore.getState().setTheme('dark')
    expect(document.documentElement).toHaveClass('dark')

    useThemeStore.getState().toggleTheme()
    expect(useThemeStore.getState().theme).toBe('light')
    expect(document.documentElement).not.toHaveClass('dark')

    useThemeStore.getState().toggleTheme()
    expect(useThemeStore.getState().theme).toBe('dark')

    useConfigStore.setState({ theme: 'light' })
    initializeTheme()
    expect(document.documentElement).not.toHaveClass('dark')
  })
})
