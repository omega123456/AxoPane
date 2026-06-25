import type { ColumnKey } from '@/lib/types/ipc'

export const columnDefinitions: Record<
  ColumnKey,
  { label: string; className: string; align?: 'left' | 'right' }
> = {
  name: { label: 'Name', className: 'min-w-0 flex-1' },
  size: { label: 'Size', className: 'w-sizecol shrink-0 text-right', align: 'right' },
  items: { label: 'Items', className: 'w-itemcol shrink-0 text-right', align: 'right' },
  type: { label: 'Type', className: 'w-typecol shrink-0' },
  modified: { label: 'Modified', className: 'w-modcol shrink-0' },
  created: { label: 'Created', className: 'w-modcol shrink-0' },
}

export const columnOrder = Object.keys(columnDefinitions) as ColumnKey[]
