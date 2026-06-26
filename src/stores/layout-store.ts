import { create } from 'zustand'
import { columnOrder } from '@/lib/columns'
import type { ColumnConfig, ColumnKey, LayoutConfig, ZoomLevel } from '@/lib/types/ipc'

export const zoomLevels: ZoomLevel[] = ['80', '90', '100', '110', '120', '125', '150']

function applyZoom(zoom: ZoomLevel) {
  if (typeof document === 'undefined') {
    return
  }
  document.documentElement.style.setProperty('zoom', String(Number(zoom) / 100))
}

export const defaultColumns: ColumnConfig[] = [
  { key: 'name', visible: true },
  { key: 'size', visible: true },
  { key: 'items', visible: true },
  { key: 'type', visible: true },
  { key: 'modified', visible: true },
  { key: 'created', visible: false },
]

export const defaultLayout: LayoutConfig = {
  detailsVisible: false,
  treeWidth: 'default',
  defaultPaneMode: 'dual',
  restoreSession: true,
  zoom: '100',
}

type LayoutStore = LayoutConfig & {
  columns: ColumnConfig[]
  hydrate: (layout: LayoutConfig, columns: ColumnConfig[]) => void
  setDetailsVisible: (visible: boolean) => void
  setTreeWidth: (width: LayoutConfig['treeWidth']) => void
  setDefaultPaneMode: (mode: LayoutConfig['defaultPaneMode']) => void
  setRestoreSession: (restoreSession: boolean) => void
  setZoom: (zoom: ZoomLevel) => void
  setColumns: (columns: ColumnConfig[]) => void
  moveColumn: (fromKey: ColumnKey, toKey: ColumnKey) => void
  toggleColumn: (key: ColumnKey) => void
  reset: () => void
}

function normalizeColumns(columns: ColumnConfig[]) {
  // Preserve the user-defined order as given, deduping by key, then append any
  // schema columns that were missing (e.g. added in a newer version) so every
  // column key is always present without discarding a saved reorder.
  const seen = new Set<ColumnKey>()
  const ordered: ColumnConfig[] = []
  for (const column of columns) {
    if (columnOrder.includes(column.key) && !seen.has(column.key)) {
      seen.add(column.key)
      ordered.push(column)
    }
  }
  for (const key of columnOrder) {
    if (!seen.has(key)) {
      ordered.push({
        key,
        visible: defaultColumns.find((column) => column.key === key)?.visible ?? true,
      })
    }
  }
  return ordered
}

export const useLayoutStore = create<LayoutStore>((set) => ({
  ...defaultLayout,
  columns: defaultColumns,
  hydrate: (layout, columns) => {
    applyZoom(layout.zoom)
    set({ ...layout, columns: normalizeColumns(columns) })
  },
  setDetailsVisible: (detailsVisible) => set({ detailsVisible }),
  setTreeWidth: (treeWidth) => set({ treeWidth }),
  setDefaultPaneMode: (defaultPaneMode) => set({ defaultPaneMode }),
  setRestoreSession: (restoreSession) => set({ restoreSession }),
  setZoom: (zoom) => {
    applyZoom(zoom)
    set({ zoom })
  },
  setColumns: (columns) => set({ columns: normalizeColumns(columns) }),
  moveColumn: (fromKey, toKey) => {
    set((state) => {
      const columns = [...state.columns]
      const fromIndex = columns.findIndex((column) => column.key === fromKey)
      const toIndex = columns.findIndex((column) => column.key === toKey)
      if (fromIndex === -1 || toIndex === -1 || fromIndex === toIndex) {
        return state
      }

      const [moved] = columns.splice(fromIndex, 1)
      columns.splice(toIndex, 0, moved)
      return { columns }
    })
  },
  toggleColumn: (key) =>
    set((state) => ({
      columns: state.columns.map((column) =>
        column.key === key ? { ...column, visible: !column.visible } : column,
      ),
    })),
  reset: () => {
    applyZoom(defaultLayout.zoom)
    set({ ...defaultLayout, columns: defaultColumns })
  },
}))
