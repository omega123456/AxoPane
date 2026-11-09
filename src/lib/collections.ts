/** A new record with `keys` set to `true` when `flagged`, or removed when not. */
export function withKeyFlags(
  record: Readonly<Record<string, true>>,
  keys: readonly string[],
  flagged: boolean,
): Record<string, true> {
  return flagged ? withKeysFlagged(record, keys) : withoutKeys(record, keys)
}

function withKeysFlagged(
  record: Readonly<Record<string, true>>,
  keys: readonly string[],
): Record<string, true> {
  const next = { ...record }
  for (const key of keys) {
    next[key] = true
  }
  return next
}

export function withoutKeys<Value>(
  record: Readonly<Record<string, Value>>,
  keys: readonly string[],
): Record<string, Value> {
  const next = { ...record }
  for (const key of keys) {
    delete next[key]
  }
  return next
}

export function withoutKey<Value>(
  record: Readonly<Record<string, Value>>,
  key: string,
): Record<string, Value> {
  const next = { ...record }
  delete next[key]
  return next
}

/** A copy of `items` with the item at `fromIndex` moved to `toIndex`. */
export function moveItem<Item>(items: readonly Item[], fromIndex: number, toIndex: number): Item[] {
  const next = [...items]
  const [moved] = next.splice(fromIndex, 1)
  next.splice(toIndex, 0, moved)
  return next
}

export function lookupOwnProperty<Value>(
  table: Readonly<Partial<Record<string, Value>>>,
  key: string,
): Value | undefined {
  return Object.hasOwn(table, key) ? table[key] : undefined
}
