import type { ColumnKey } from '@/lib/types/ipc'

export type ColumnAlign = 'left' | 'right'

export const columnDefinitions: Record<
  ColumnKey,
  { label: string; className: string; align?: ColumnAlign }
> = {
  name: { label: 'Name', className: 'min-w-0' },
  size: { label: 'Size', className: 'shrink-0 pr-3 text-right', align: 'right' },
  items: { label: 'Items', className: 'shrink-0 pr-3 text-right', align: 'right' },
  type: { label: 'Type', className: 'shrink-0 pl-3' },
  modified: { label: 'Modified', className: 'shrink-0 pl-3' },
  created: { label: 'Created', className: 'shrink-0 pl-3' },
}

export const columnOrder = Object.keys(columnDefinitions) as ColumnKey[]

export function columnCellClassName(key: ColumnKey) {
  return `${columnDefinitions[key].className} truncate`
}
