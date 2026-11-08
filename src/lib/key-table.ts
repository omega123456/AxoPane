import { lookupOwnProperty } from '@/lib/collections'

export type KeyHandlerTable = Partial<Record<string, () => void>>

type KeyEventLike = { key: string; preventDefault: () => void }

type ModifierKeyState = { ctrlKey: boolean; metaKey: boolean }

export function hasPrimaryModifier(event: ModifierKeyState) {
  return event.ctrlKey || event.metaKey
}

/** Runs the handler mapped to `event.key`, preventing the default when one exists. */
export function handleKeyByTable(event: KeyEventLike, table: KeyHandlerTable): boolean {
  const handler = lookupOwnProperty(table, event.key)
  if (!handler) {
    return false
  }
  event.preventDefault()
  handler()
  return true
}

export function caseInsensitiveLetterKeys(table: KeyHandlerTable): KeyHandlerTable {
  const bothCases: KeyHandlerTable = {}
  for (const [letter, handler] of Object.entries(table)) {
    bothCases[letter.toLowerCase()] = handler
    bothCases[letter.toUpperCase()] = handler
  }
  return bothCases
}
