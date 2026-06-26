import { describe, expect, it } from 'vitest'
import {
  captureShortcut,
  defaultKeymap,
  findKeybindingConflicts,
  formatShortcutLabel,
  isReservedCommand,
  migrateKeymap,
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

    const copyToOtherPane = new KeyboardEvent('keydown', { key: 'F5' })
    expect(resolveCommandForEvent(copyToOtherPane, defaultKeymap)).toBe('copyToOtherPane')

    const copyToClipboard = new KeyboardEvent('keydown', { key: 'c', ctrlKey: true })
    expect(resolveCommandForEvent(copyToClipboard, defaultKeymap)).toBe('copy')
  })

  it('merges defaults and detects conflicts', () => {
    const merged = mergeKeymap({ rename: ['Ctrl+R'] })
    expect(merged.open).toEqual(['Enter'])
    expect(findKeybindingConflicts(merged)).toContainEqual(['Ctrl+R', ['refresh', 'rename']])
  })

  it('migrates the legacy refresh F5 binding back to the current default', () => {
    const migrated = migrateKeymap({ refresh: ['F5'] })
    expect(migrated.migrated).toBe(true)
    expect(migrated.bindings.refresh).toEqual(['Ctrl+R'])
    expect(findKeybindingConflicts(migrated.bindings)).toEqual([])
  })

  it('forces reserved clipboard commands to OS defaults and keeps them off the conflict checker', () => {
    expect(isReservedCommand('copy')).toBe(true)
    expect(isReservedCommand('cut')).toBe(true)
    expect(isReservedCommand('paste')).toBe(true)
    expect(isReservedCommand('rename')).toBe(false)

    // A stale persisted override for a reserved command is ignored: it always
    // resolves back to the platform default and never surfaces a conflict.
    const merged = mergeKeymap({ copy: ['Ctrl+Shift+C'], copyToOtherPane: ['Ctrl+C'] })
    expect(merged.copy).toEqual(['Ctrl+C'])
    expect(findKeybindingConflicts(merged)).toEqual([])
  })

  it('prefers the current default owner when a stale conflict still exists', () => {
    const event = new KeyboardEvent('keydown', { key: 'F5' })
    const conflicted = mergeKeymap({ refresh: ['F5'] })

    expect(resolveCommandForEvent(event, conflicted)).toBe('copyToOtherPane')
  })
})
