import { beforeEach } from 'vitest'
import { useConfigStore } from '@/stores/config-store'
import { useLayoutStore } from '@/stores/layout-store'
import { clearSelectionForPane, useSelectionStore } from '@/stores/selection-store'
import { initializeTheme, useThemeStore } from '@/stores/theme-store'

beforeEach(() => {
  useConfigStore.getState().reset()
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
