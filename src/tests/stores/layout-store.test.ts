import { describe, expect, it } from 'vitest'
import {
  COLUMN_WIDTH_MAX,
  defaultColumns,
  defaultColumnWidths,
  defaultLayout,
  hydratedLayout,
  TREE_WIDTH_MIN,
  withColumnWidth,
  withToggledColumn,
} from '@/stores/layout-store'

describe('hydratedLayout', () => {
  it('clamps sizes and fills in missing columns', () => {
    const hydrated = hydratedLayout({ ...defaultLayout, treeWidthPx: 1 }, [
      { key: 'size', visible: false },
    ])
    expect(hydrated.treeWidthPx).toBe(TREE_WIDTH_MIN)
    expect(hydrated.columns[0]).toEqual({ key: 'size', visible: false })
    expect(hydrated.columns).toHaveLength(defaultColumns.length)
  })
})

describe('withColumnWidth', () => {
  it('sets one clamped column width and keeps the others', () => {
    const next = withColumnWidth(
      { columnWidths: defaultColumnWidths },
      { key: 'name', width: COLUMN_WIDTH_MAX + 1 },
    )
    expect(next.columnWidths.name).toBe(COLUMN_WIDTH_MAX)
    expect(next.columnWidths.size).toBe(defaultColumnWidths.size)
  })
})

describe('withToggledColumn', () => {
  it('flips the visibility of only the named column', () => {
    const next = withToggledColumn({ columns: defaultColumns }, 'created')
    expect(next.columns.find((column) => column.key === 'created')?.visible).toBe(true)
    expect(next.columns.find((column) => column.key === 'name')?.visible).toBe(true)
  })
})
