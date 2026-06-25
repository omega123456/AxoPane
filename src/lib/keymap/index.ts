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
  copy: 'Copy',
  cut: 'Cut',
  paste: 'Paste',
  newFolder: 'New folder',
  newFile: 'New file',
  calculateSize: 'Calculate size',
  openInNewTab: 'Open in new tab',
  openInOtherPane: 'Open in other pane',
  selectAll: 'Select all',
  clearFilter: 'Clear filter',
  toggleDetails: 'Toggle details',
  showSettings: 'Settings',
}

export const defaultKeymap: Record<CommandId, Shortcut[]> = {
  open: ['Enter'],
  goUp: ['Backspace'],
  refresh: ['Ctrl+R'],
  rename: ['F2'],
  delete: ['Delete'],
  copy: ['Ctrl+C', 'F5'],
  cut: ['Ctrl+X', 'F6'],
  paste: ['Ctrl+V'],
  newFolder: ['Ctrl+Shift+N'],
  newFile: [],
  calculateSize: ['Space'],
  openInNewTab: ['Ctrl+Enter'],
  openInOtherPane: ['Ctrl+Shift+Enter'],
  selectAll: ['Ctrl+A'],
  clearFilter: ['Escape'],
  toggleDetails: ['Alt+Enter'],
  showSettings: ['Ctrl+,'],
}

const modifierOrder = ['Ctrl', 'Meta', 'Alt', 'Shift'] as const
const modifierLabels: Record<PlatformOs, Record<'Ctrl' | 'Meta' | 'Alt' | 'Shift', string>> = {
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

export function normalizeShortcut(value: string): Shortcut {
  const parts = value
    .split('+')
    .map((part) => part.trim())
    .filter(Boolean)

  if (parts.length === 0) {
    return ''
  }

  const modifiers = new Set<string>()
  let key = ''

  for (const part of parts) {
    const lower = part.toLowerCase()
    if (lower === 'control' || lower === 'ctrl') {
      modifiers.add('Ctrl')
    } else if (lower === 'cmd' || lower === 'command' || lower === 'meta') {
      modifiers.add('Meta')
    } else if (lower === 'option' || lower === 'alt') {
      modifiers.add('Alt')
    } else if (lower === 'shift') {
      modifiers.add('Shift')
    } else if (lower === ' ') {
      key = 'Space'
    } else if (lower === 'escape' || lower === 'esc') {
      key = 'Escape'
    } else if (lower === 'arrowup' || lower === 'arrowdown' || lower === 'arrowleft' || lower === 'arrowright') {
      key = `Arrow${lower.slice(5, 6).toUpperCase()}${lower.slice(6)}`
    } else if (lower.length === 1) {
      key = lower.toUpperCase()
    } else {
      key = part[0].toUpperCase() + part.slice(1)
    }
  }

  const ordered = modifierOrder.filter((modifier) => modifiers.has(modifier))
  return [...ordered, key].filter(Boolean).join('+')
}

function isModifierOnly(value: string) {
  return value === 'Ctrl' || value === 'Meta' || value === 'Alt' || value === 'Shift'
}

function normalizeEventKey(value: string) {
  if (value === ' ') {
    return 'Space'
  }
  if (value === 'Esc') {
    return 'Escape'
  }
  if (value === 'Control') {
    return 'Ctrl'
  }
  if (value === 'Meta') {
    return 'Meta'
  }
  if (value === 'Alt') {
    return 'Alt'
  }
  if (value === 'Shift') {
    return 'Shift'
  }
  if (value === 'OS') {
    return 'Meta'
  }
  if (value.length === 1) {
    return value.toUpperCase()
  }
  return value
}

export function captureShortcut(event: KeyboardEvent): Shortcut | null {
  if (event.key === 'Tab') {
    return null
  }

  const key = normalizeEventKey(event.key)
  if (!key) {
    return null
  }

  const parts: string[] = []
  if (event.ctrlKey) {
    parts.push('Ctrl')
  }
  if (event.metaKey) {
    parts.push('Meta')
  }
  if (event.altKey) {
    parts.push('Alt')
  }
  if (event.shiftKey) {
    parts.push('Shift')
  }

  if (parts.length === 0 && isModifierOnly(key)) {
    return null
  }

  return normalizeShortcut([...parts, key].join('+'))
}

export function parseShortcut(shortcut: Shortcut): ShortcutParts {
  const normalized = normalizeShortcut(shortcut)
  const parts = normalized.split('+').filter(Boolean)
  const key = parts.find((part) => !modifierOrder.includes(part as (typeof modifierOrder)[number])) ?? ''

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

export function formatShortcutLabel(shortcut: Shortcut, os: PlatformOs) {
  const parts = normalizeShortcut(shortcut).split('+').filter(Boolean)
  return parts
    .map((part) => {
      if (part === 'Ctrl' || part === 'Meta' || part === 'Alt' || part === 'Shift') {
        return modifierLabels[os][part]
      }
      return part === 'Space' ? 'Space' : part
    })
    .join(os === 'macos' ? '' : '+')
}

export function mergeKeymap(overrides: Partial<Record<CommandId, Shortcut[]>>) {
  return Object.fromEntries(
    Object.entries(defaultKeymap).map(([commandId, defaults]) => [
      commandId,
      (overrides[commandId as CommandId] ?? defaults).map(normalizeShortcut).filter(Boolean),
    ]),
  ) as Record<CommandId, Shortcut[]>
}

export function findKeybindingConflicts(keymap: Record<CommandId, Shortcut[]>) {
  const seen = new Map<Shortcut, CommandId[]>()
  for (const [commandId, shortcuts] of Object.entries(keymap) as [CommandId, Shortcut[]][]) {
    for (const shortcut of shortcuts) {
      const normalized = normalizeShortcut(shortcut)
      if (!normalized) {
        continue
      }
      seen.set(normalized, [...(seen.get(normalized) ?? []), commandId])
    }
  }

  return Array.from(seen.entries()).filter(([, commandIds]) => commandIds.length > 1)
}

export function resolveCommandForEvent(
  event: KeyboardEvent,
  keymap: Record<CommandId, Shortcut[]>,
): CommandId | null {
  for (const commandId of Object.keys(keymap) as CommandId[]) {
    if (keymap[commandId].some((shortcut) => eventMatchesShortcut(event, shortcut))) {
      return commandId
    }
  }

  return null
}
