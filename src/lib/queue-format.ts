import { formatCount } from '@/lib/format'
import type { OpProgress } from '@/lib/types/ipc'

/** Present-participle verb for an operation's kind, e.g. `Copying` / `Deleting`. */
export function verb(operation: OpProgress) {
  if (operation.kind === 'delete') {
    return 'Deleting'
  }
  if (operation.kind === 'compress') {
    return 'Compressing'
  }
  if (operation.kind === 'extract') {
    return 'Extracting'
  }
  return operation.kind === 'move' ? 'Moving' : 'Copying'
}

/**
 * Joins up to the first two item names, appending a "+K more" suffix once
 * `totalItems` exceeds that preview — e.g. `"a.txt, b.txt, +3 more"`.
 * Returns `null` when there are no item names to preview.
 */
export function formatItemPreview(itemNames: string[], totalItems: number): string | null {
  if (itemNames.length === 0) {
    return null
  }
  if (totalItems <= 2) {
    return itemNames.join(', ')
  }
  return `${itemNames.slice(0, 2).join(', ')}, +${formatCount(totalItems - 2)} more`
}
