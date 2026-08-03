import type { ColumnKey } from '@/lib/types/ipc'

export const columnDefinitions: Record<
  ColumnKey,
  { label: string; className: string; align?: 'left' | 'right' }
> = {
  name: { label: 'Name', className: 'min-w-0' },
  size: { label: 'Size', className: 'shrink-0 text-right', align: 'right' },
  items: { label: 'Items', className: 'shrink-0 text-right', align: 'right' },
  type: { label: 'Type', className: 'shrink-0' },
  modified: { label: 'Modified', className: 'shrink-0' },
  created: { label: 'Created', className: 'shrink-0' },
}

export const columnOrder = Object.keys(columnDefinitions) as ColumnKey[]

/**
 * Class for a data row's fixed-width cell wrapper. `truncate` has to live on the
 * wrapper (a blockified flex item) — on the inline value spans inside it,
 * `overflow: hidden` is inert and long values bleed into the next column.
 */
export function columnCellClassName(key: ColumnKey) {
  return `${columnDefinitions[key].className} truncate`
}
