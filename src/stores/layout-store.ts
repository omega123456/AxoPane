import { create } from 'zustand'
import { columnOrder } from '@/lib/columns'
import type { ColumnConfig, ColumnKey, LayoutConfig, ZoomLevel } from '@/lib/types/ipc'

export const zoomLevels: ZoomLevel[] = ['80', '90', '100', '110', '120', '125', '150']

function applyZoom(zoom: ZoomLevel) {
  if (typeof document === 'undefined') {
    return
  }
  const factor = Number(zoom) / 100
  const root = document.documentElement
  root.style.setProperty('zoom', String(factor))
  // `zoom` scales rendered size but `100vh` still resolves to the unzoomed
  // viewport, so a zoomed root would render taller than the window and scroll
  // the whole page (pane/tree internal scrolling never engages). Counter-scale
  // the root height so its rendered height (height * factor) stays exactly one
  // viewport, then chain an explicit height through body/#root so the app shell
  // fills it. Runtime geometry only — no static token can express 100/factor.
  root.style.height = `${100 / factor}vh`
  document.body.style.height = '100%'
  const appRoot = document.getElementById('root')
  if (appRoot) {
    appRoot.style.height = '100%'
  }
}

export const defaultColumns: ColumnConfig[] = [
  { key: 'name', visible: true },
  { key: 'size', visible: true },
  { key: 'items', visible: true },
  { key: 'type', visible: true },
  { key: 'modified', visible: true },
  { key: 'created', visible: false },
]

/** Folder-tree sidebar drag bounds, in pixels. */
export const TREE_WIDTH_MIN = 160
export const TREE_WIDTH_MAX = 480
/** Dual-pane split drag bounds, as the fraction allotted to the left pane. */
export const PANE_SPLIT_MIN = 0.2
export const PANE_SPLIT_MAX = 0.8

export function clampTreeWidth(width: number) {
  return Math.min(TREE_WIDTH_MAX, Math.max(TREE_WIDTH_MIN, width))
}

export function clampPaneSplit(split: number) {
  return Math.min(PANE_SPLIT_MAX, Math.max(PANE_SPLIT_MIN, split))
}

export const defaultLayout: LayoutConfig = {
  detailsVisible: false,
  treeWidthPx: 204,
  paneSplit: 0.5,
  defaultPaneMode: 'dual',
  restoreSession: true,
  zoom: '100',
}

type LayoutStore = LayoutConfig & {
  columns: ColumnConfig[]
  hydrate: (layout: LayoutConfig, columns: ColumnConfig[]) => void
  setDetailsVisible: (visible: boolean) => void
  setTreeWidthPx: (width: number) => void
  setPaneSplit: (split: number) => void
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
    set({
      ...layout,
      treeWidthPx: clampTreeWidth(layout.treeWidthPx),
      paneSplit: clampPaneSplit(layout.paneSplit),
      columns: normalizeColumns(columns),
    })
  },
  setDetailsVisible: (detailsVisible) => set({ detailsVisible }),
  setTreeWidthPx: (width) => set({ treeWidthPx: clampTreeWidth(width) }),
  setPaneSplit: (split) => set({ paneSplit: clampPaneSplit(split) }),
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
