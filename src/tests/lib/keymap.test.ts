import { describe, expect, it } from 'vitest'
import {
  captureShortcut,
  defaultKeymap,
  findKeybindingConflicts,
  formatShortcutLabel,
  mergeKeymap,
  normalizeShortcut,
  resolveCommandForEvent,
} from '@/lib/keymap'

describe('keymap', () => {
  it('normalizes and formats shortcuts from a shared source of truth', () => {
    expect(normalizeShortcut('cmd+,')).toBe('Meta+,')
    expect(formatShortcutLabel('Ctrl+R', 'windows')).toBe('Ctrl+R')
    // The stored primary modifier (Ctrl) renders as ⌘ on macOS.
    expect(formatShortcutLabel('Ctrl+,', 'macos')).toBe('⌘,')
    // The literal Meta key (never used in defaults) renders as ⌃ on macOS.
    expect(formatShortcutLabel('Meta+,', 'macos')).toBe('⌃,')
  })

  it('captures keyboard chords and resolves commands', () => {
    const event = new KeyboardEvent('keydown', { key: 'r', ctrlKey: true })
    expect(captureShortcut(event)).toBe('Ctrl+R')
    expect(resolveCommandForEvent(event, defaultKeymap)).toBe('refresh')
  })

  it('merges defaults and detects conflicts', () => {
    const merged = mergeKeymap({ rename: ['Ctrl+R'] })
    expect(merged.open).toEqual(['Enter'])
    expect(findKeybindingConflicts(merged)).toContainEqual(['Ctrl+R', ['refresh', 'rename']])
  })
})
