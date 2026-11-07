/** A new record with `keys` set to `true` when `flagged`, or removed when not. */
export function withKeyFlags(
  record: Readonly<Record<string, true>>,
  keys: readonly string[],
  flagged: boolean,
): Record<string, true> {
  const next = { ...record }
  for (const key of keys) {
    if (flagged) {
      next[key] = true
    } else {
      delete next[key]
    }
  }
  return next
}

/** A copy of `items` with the item at `fromIndex` moved to `toIndex`. */
export function moveItem<Item>(items: readonly Item[], fromIndex: number, toIndex: number): Item[] {
  const next = [...items]
  const [moved] = next.splice(fromIndex, 1)
  next.splice(toIndex, 0, moved)
  return next
}
