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
