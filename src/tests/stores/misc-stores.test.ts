import { beforeEach, vi } from 'vitest'
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
    useLayoutStore.getState().setTreeWidth('wide')
    useLayoutStore.getState().setDefaultPaneMode('single')
    useLayoutStore.getState().setRestoreSession(false)
    useLayoutStore.getState().setZoom('125')
    useLayoutStore.getState().toggleColumn('created')

    expect(useLayoutStore.getState()).toMatchObject({
      treeWidth: 'wide',
      defaultPaneMode: 'single',
      restoreSession: false,
      zoom: '125',
    })
    expect(setProperty).toHaveBeenCalledWith('zoom', '1.25')
    setProperty.mockRestore()
    expect(useLayoutStore.getState().columns.find((column) => column.key === 'created')).toEqual({
      key: 'created',
      visible: true,
    })
  })
})

describe('context-menu-store', () => {
  it('opens with hidden items removed and focuses the first enabled item', () => {
    useContextMenuStore.getState().openMenu({
      paneId: 'left',
      x: 10,
      y: 20,
      title: 'Entry',
      items: [
        { id: 'hidden', label: 'Hidden', hidden: true },
        { id: 'disabled', label: 'Disabled', disabled: true },
        { id: 'open', label: 'Open' },
      ],
    })

    const state = useContextMenuStore.getState()
    expect(state.menu?.items.map((item) => item.id)).toEqual(['disabled', 'open'])
    expect(state.activeIndex).toBe(1)
  })

  it('wraps active movement across enabled items and skips disabled items', () => {
    useContextMenuStore.getState().openMenu({
      paneId: 'left',
      x: 0,
      y: 0,
      title: 'Entry',
      items: [
        { id: 'open', label: 'Open' },
        { id: 'disabled', label: 'Disabled', disabled: true },
        { id: 'rename', label: 'Rename' },
      ],
    })

    useContextMenuStore.getState().moveActive(1)
    expect(useContextMenuStore.getState().activeIndex).toBe(2)
    useContextMenuStore.getState().moveActive(1)
    expect(useContextMenuStore.getState().activeIndex).toBe(0)
    useContextMenuStore.getState().moveActive(-1)
    expect(useContextMenuStore.getState().activeIndex).toBe(2)
  })

  it('does not move or activate when no enabled menu item is available', () => {
    useContextMenuStore.getState().openMenu({
      paneId: 'right',
      x: 0,
      y: 0,
      title: 'Entry',
      items: [{ id: 'disabled', label: 'Disabled', disabled: true }],
    })

    useContextMenuStore.getState().moveActive(1)
    useContextMenuStore.getState().activateCurrent()

    expect(useContextMenuStore.getState().menu?.items).toHaveLength(1)
    expect(useContextMenuStore.getState().activeIndex).toBe(0)
  })

  it('activates the current item and closes the menu', () => {
    const onSelect = vi.fn()
    useContextMenuStore.getState().openMenu({
      paneId: 'left',
      x: 0,
      y: 0,
      title: 'Entry',
      items: [{ id: 'open', label: 'Open', onSelect }],
    })

    useContextMenuStore.getState().activateCurrent()

    expect(onSelect).toHaveBeenCalledOnce()
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
