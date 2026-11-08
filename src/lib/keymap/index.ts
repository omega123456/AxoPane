import { lookupOwnProperty } from '@/lib/collections'
import type { CommandId, Shortcut } from '@/lib/types/ipc'

export type PlatformOs = 'windows' | 'macos'
export type ShortcutParts = {
  ctrl: boolean
  meta: boolean
  alt: boolean
  shift: boolean
  key: string
}

export const commandLabels: Record<CommandId, string> = {
  open: 'Open',
  goUp: 'Go up',
  refresh: 'Refresh',
  rename: 'Rename',
  delete: 'Delete',
  deletePermanent: 'Delete permanently',
  restore: 'Restore',
  emptyTrash: 'Empty Trash',
  copy: 'Copy',
  cut: 'Cut',
  paste: 'Paste',
  copyToOtherPane: 'Copy to other pane',
  moveToOtherPane: 'Move to other pane',
  newFolder: 'New folder',
  newFile: 'New file',
  calculateSize: 'Calculate size',
  openInNewTab: 'Open in new tab',
  openInOtherPane: 'Open in other pane',
  selectAll: 'Select all',
  clearFilter: 'Clear filter',
  showSettings: 'Settings',
  searchEverywhere: 'Search everywhere',
}

// MacBook keyboards have no forward-delete key (the labelled "delete" key emits
// Backspace, already bound to Go up), so macOS defaults to F8 / Shift+F8 while
// still listening to the physical Delete keys present on full-size keyboards.
// Windows keeps the conventional Delete / Shift+Delete. The platform-primary
// binding is listed first so the Settings UI surfaces the right one.
const deleteDefaults: { delete: Shortcut[]; deletePermanent: Shortcut[] } =
  detectPlatformOs() === 'macos'
    ? { delete: ['F8', 'Delete'], deletePermanent: ['Shift+F8', 'Shift+Delete'] }
    : { delete: ['Delete'], deletePermanent: ['Shift+Delete'] }

// F4 is a media/Spotlight key on most Mac keyboards unless Fn is held, so
// macOS also gets ⌘⇧F ("Ctrl" renders as ⌘ there) for Search everywhere.
const searchEverywhereDefaults: Shortcut[] =
  detectPlatformOs() === 'macos' ? ['F4', 'Ctrl+Shift+F'] : ['F4']

export const defaultKeymap: Record<CommandId, Shortcut[]> = {
  open: ['Enter'],
  goUp: ['Backspace'],
  refresh: ['Ctrl+R'],
  rename: ['F2'],
  delete: deleteDefaults.delete,
  deletePermanent: deleteDefaults.deletePermanent,
  restore: [],
  emptyTrash: [],
  copy: ['Ctrl+C'],
  cut: ['Ctrl+X'],
  paste: ['Ctrl+V'],
  copyToOtherPane: ['F5'],
  moveToOtherPane: ['F6'],
  newFolder: ['Ctrl+Shift+N'],
  newFile: [],
  calculateSize: ['Space'],
  openInNewTab: ['Ctrl+Enter'],
  openInOtherPane: ['Ctrl+Shift+Enter'],
  selectAll: ['Ctrl+A'],
  clearFilter: ['Escape'],
  showSettings: ['Ctrl+,'],
  searchEverywhere: searchEverywhereDefaults,
}

// Clipboard commands are bound to the platform's native editing shortcuts
// (Ctrl/⌘+C/X/V) and are intentionally NOT remappable. Treating them as the OS
// default keeps them off the conflict checker (they previously surfaced spurious
// "Conflict" badges when a stale persisted override shadowed them) and stops the
// Settings UI from offering a rebind that would fight the platform.
export const reservedCommands: ReadonlySet<CommandId> = new Set(['copy', 'cut', 'paste'])

export function isReservedCommand(commandId: CommandId) {
  return reservedCommands.has(commandId)
}

type KeymapMigration = {
  commandId: CommandId
  from: Shortcut[]
  to: Shortcut[]
}

const legacyKeymapMigrations: KeymapMigration[] = [
  {
    // Earlier builds used F5 for refresh. Once cross-pane copy moved onto F5,
    // persisted configs could shadow the new default indefinitely.
    commandId: 'refresh',
    from: ['F5'],
    to: defaultKeymap.refresh,
  },
]

const modifierOrder = ['Ctrl', 'Meta', 'Alt', 'Shift'] as const
type Modifier = (typeof modifierOrder)[number]

const modifierLabels: Record<PlatformOs, Record<Modifier, string>> = {
  windows: {
    Ctrl: 'Ctrl',
    Meta: 'Win',
    Alt: 'Alt',
    Shift: 'Shift',
  },
  macos: {
    // Shortcuts are stored with `Ctrl` as the single source-of-truth primary
    // modifier. On macOS the primary modifier is Command, so render `Ctrl` as
    // ⌘ (and the literal Meta key, never used in defaults, as ⌃).
    Ctrl: '⌘',
    Meta: '⌃',
    Alt: '⌥',
    Shift: '⇧',
  },
}

export function detectPlatformOs(): PlatformOs {
  if (typeof navigator === 'undefined') {
    return 'windows'
  }

  return /Mac|iPhone|iPad|iPod/.test(navigator.platform) ? 'macos' : 'windows'
}

const platformCommandModifier: Record<PlatformOs, Modifier> = {
  macos: 'Ctrl',
  windows: 'Meta',
}

const fixedModifierAliases: Readonly<Record<string, Modifier>> = {
  control: 'Ctrl',
  ctrl: 'Ctrl',
  meta: 'Meta',
  option: 'Alt',
  alt: 'Alt',
  shift: 'Shift',
}

const commandModifierAliases: ReadonlySet<string> = new Set(['cmd', 'command'])

const keyNameAliases: Readonly<Record<string, string>> = {
  ' ': 'Space',
  escape: 'Escape',
  esc: 'Escape',
  arrowup: 'ArrowUp',
  arrowdown: 'ArrowDown',
  arrowleft: 'ArrowLeft',
  arrowright: 'ArrowRight',
}

function shortcutModifierFor(lowerPart: string, platform: PlatformOs): Modifier | undefined {
  if (commandModifierAliases.has(lowerPart)) {
    return platformCommandModifier[platform]
  }
  return lookupOwnProperty(fixedModifierAliases, lowerPart)
}

function capitalize(text: string) {
  return text[0].toUpperCase() + text.slice(1)
}

function normalizeKeyName(part: string) {
  const lowerPart = part.toLowerCase()
  const alias = lookupOwnProperty(keyNameAliases, lowerPart)
  if (alias) {
    return alias
  }
  return lowerPart.length === 1 ? lowerPart.toUpperCase() : capitalize(part)
}

export function normalizeShortcut(value: string): Shortcut {
  const platform = detectPlatformOs()
  const parts = value
    .split('+')
    .map((part) => part.trim())
    .filter(Boolean)

  if (parts.length === 0) {
    return ''
  }

  const modifiers = new Set<Modifier>()
  let key = ''

  for (const part of parts) {
    const modifier = shortcutModifierFor(part.toLowerCase(), platform)
    if (modifier) {
      modifiers.add(modifier)
      continue
    }
    key = normalizeKeyName(part)
  }

  const ordered = modifierOrder.filter((modifier) => modifiers.has(modifier))
  return [...ordered, key].filter(Boolean).join('+')
}

function isModifierOnly(value: string): value is Modifier {
  return (modifierOrder as readonly string[]).includes(value)
}

const eventKeyAliases: Record<PlatformOs, Readonly<Record<string, string>>> = {
  macos: { ' ': 'Space', Esc: 'Escape', Control: 'Meta', Meta: 'Ctrl', OS: 'Ctrl' },
  windows: { ' ': 'Space', Esc: 'Escape', Control: 'Ctrl', Meta: 'Meta', OS: 'Meta' },
}

function normalizeEventKey(value: string, platform: PlatformOs) {
  const alias = lookupOwnProperty(eventKeyAliases[platform], value)
  if (alias) {
    return alias
  }
  return value.length === 1 ? value.toUpperCase() : value
}

function eventModifierParts(event: KeyboardEvent, platform: PlatformOs) {
  const isMac = platform === 'macos'
  const modifiers = [
    { part: 'Ctrl', pressed: isMac ? event.metaKey : event.ctrlKey },
    { part: 'Meta', pressed: isMac ? event.ctrlKey : event.metaKey },
    { part: 'Alt', pressed: event.altKey },
    { part: 'Shift', pressed: event.shiftKey },
  ]
  return modifiers.filter((modifier) => modifier.pressed).map((modifier) => modifier.part)
}

export function captureShortcut(event: KeyboardEvent): Shortcut | null {
  const platform = detectPlatformOs()
  if (event.key === 'Tab') {
    return null
  }

  const key = normalizeEventKey(event.key, platform)
  if (!key) {
    return null
  }

  if (isModifierOnly(key)) {
    return null
  }

  const parts = eventModifierParts(event, platform)
  return normalizeShortcut([...parts, key].join('+'))
}

export function parseShortcut(shortcut: Shortcut): ShortcutParts {
  const normalized = normalizeShortcut(shortcut)
  const parts = normalized.split('+').filter(Boolean)
  const key = parts.find((part) => !isModifierOnly(part)) ?? ''

  return {
    ctrl: parts.includes('Ctrl'),
    meta: parts.includes('Meta'),
    alt: parts.includes('Alt'),
    shift: parts.includes('Shift'),
    key,
  }
}

export function eventMatchesShortcut(event: KeyboardEvent, shortcut: Shortcut) {
  return captureShortcut(event) === normalizeShortcut(shortcut)
}

export function formatShortcutLabel(shortcut: Shortcut, platform: PlatformOs) {
  const parts = normalizeShortcut(shortcut).split('+').filter(Boolean)
  return parts
    .map((part) => (isModifierOnly(part) ? modifierLabels[platform][part] : part))
    .join(platform === 'macos' ? '' : '+')
}

export function mergeKeymap(overrides: Partial<Record<CommandId, Shortcut[]>>) {
  return Object.fromEntries(
    Object.entries(defaultKeymap).map(([commandId, defaults]) => [
      commandId,
      // Reserved commands always use their OS default, ignoring any persisted
      // override so they can never be rebound or shadowed.
      (isReservedCommand(commandId as CommandId)
        ? defaults
        : (overrides[commandId as CommandId] ?? defaults)
      )
        .map(normalizeShortcut)
        .filter(Boolean),
    ]),
  ) as Record<CommandId, Shortcut[]>
}

function shortcutsEqual(left: Shortcut[], right: Shortcut[]) {
  if (left.length !== right.length) {
    return false
  }

  return left.every(
    (shortcut, index) => normalizeShortcut(shortcut) === normalizeShortcut(right[index]),
  )
}

export function migrateKeymap(overrides: Partial<Record<CommandId, Shortcut[]>>) {
  const nextOverrides = { ...overrides }
  let migrated = false

  for (const migration of legacyKeymapMigrations) {
    const current = (nextOverrides[migration.commandId] ?? [])
      .map(normalizeShortcut)
      .filter(Boolean)
    if (!shortcutsEqual(current, migration.from)) {
      continue
    }

    nextOverrides[migration.commandId] = [...migration.to]
    migrated = true
  }

  return {
    bindings: mergeKeymap(nextOverrides),
    migrated,
  }
}

export function findKeybindingConflicts(keymap: Record<CommandId, Shortcut[]>) {
  const seen = new Map<Shortcut, CommandId[]>()
  for (const [commandId, shortcuts] of Object.entries(keymap) as [CommandId, Shortcut[]][]) {
    if (isReservedCommand(commandId)) {
      continue
    }
    const normalizedShortcuts = shortcuts.map(normalizeShortcut).filter(Boolean)
    for (const normalized of normalizedShortcuts) {
      seen.set(normalized, [...(seen.get(normalized) ?? []), commandId])
    }
  }

  return Array.from(seen.entries()).filter(([, commandIds]) => commandIds.length > 1)
}

export function resolveCommandForEvent(
  event: KeyboardEvent,
  keymap: Record<CommandId, Shortcut[]>,
): CommandId | null {
  const matchingCommands = (Object.keys(keymap) as CommandId[]).filter((commandId) =>
    keymap[commandId].some((shortcut) => eventMatchesShortcut(event, shortcut)),
  )

  if (matchingCommands.length === 0) {
    return null
  }

  if (matchingCommands.length === 1) {
    return matchingCommands[0]
  }

  const capturedShortcut = captureShortcut(event)
  if (!capturedShortcut) {
    return matchingCommands[0]
  }

  const defaultOwner = matchingCommands.find((commandId) =>
    defaultKeymap[commandId].some(
      (shortcut) => normalizeShortcut(shortcut) === normalizeShortcut(capturedShortcut),
    ),
  )

  return defaultOwner ?? matchingCommands[0]
}
