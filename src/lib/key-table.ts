export type KeyHandlerTable = Partial<Record<string, () => void>>

type KeyEventLike = { key: string; preventDefault: () => void }

/** Runs the handler mapped to `event.key`, preventing the default when one exists. */
export function handleKeyByTable(event: KeyEventLike, table: KeyHandlerTable): boolean {
  const handler = table[event.key]
  if (!handler) {
    return false
  }
  event.preventDefault()
  handler()
  return true
}
