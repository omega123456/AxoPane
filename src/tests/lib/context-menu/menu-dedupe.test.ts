import { describe, expect, it } from 'vitest'
import { dedupeNativeMenuItems } from '@/lib/context-menu/menu-dedupe'
import type { NativeMenuItem } from '@/lib/types/ipc'

function item(overrides: Partial<NativeMenuItem>): NativeMenuItem {
  return {
    id: 'native-item',
    label: 'Native item',
    enabled: true,
    danger: false,
    canonicalActionKind: null,
    normalizedVerb: null,
    invokeToken: 'native:token',
    icon: null,
    children: [],
    ...overrides,
  }
}

describe('dedupeNativeMenuItems', () => {
  it('drops backend-tagged app-owned duplicates before rendering', () => {
    const items = dedupeNativeMenuItems([
      item({
        id: 'duplicate-open-with',
        label: 'Open with',
        canonicalActionKind: 'openWith',
      }),
      item({
        id: 'keep-terminal',
        label: 'Open in Terminal',
        normalizedVerb: 'openinterminal',
      }),
    ])

    expect(items.map((entry) => entry.id)).toEqual(['keep-terminal'])
  })

  it('uses normalized verbs and label fallback when canonical kinds are absent', () => {
    const items = dedupeNativeMenuItems([
      item({
        id: 'duplicate-properties',
        label: 'Properties',
        normalizedVerb: 'properties',
      }),
      item({
        id: 'duplicate-copy-by-label',
        label: 'Copy',
        normalizedVerb: null,
      }),
      item({
        id: 'keep-custom',
        label: 'Scan with Fixture',
        normalizedVerb: 'scanwithfixture',
      }),
    ])

    expect(items.map((entry) => entry.id)).toEqual(['keep-custom'])
  })

  it('retains submenu parents when only non-duplicate children remain', () => {
    const items = dedupeNativeMenuItems([
      item({
        id: 'archive-tools',
        label: 'Archive tools',
        invokeToken: null,
        children: [
          item({
            id: 'duplicate-compress',
            label: 'Compress',
            canonicalActionKind: 'compress',
          }),
          item({
            id: 'keep-share',
            label: 'Share with team',
            normalizedVerb: 'sharewithteam',
          }),
        ],
      }),
    ])

    expect(items).toHaveLength(1)
    expect(items[0]?.children.map((entry) => entry.id)).toEqual(['keep-share'])
  })

  it('preserves submenu containers such as New even when their labels overlap app-owned actions', () => {
    const items = dedupeNativeMenuItems([
      item({
        id: 'new-submenu',
        label: 'New',
        canonicalActionKind: 'newFolder',
        invokeToken: null,
        children: [
          item({
            id: 'new-text-document',
            label: 'Text Document',
            canonicalActionKind: null,
            normalizedVerb: 'newtextdocument',
          }),
        ],
      }),
    ])

    expect(items).toHaveLength(1)
    expect(items[0]?.label).toBe('New')
    expect(items[0]?.children.map((entry) => entry.id)).toEqual(['new-text-document'])
  })

  it('drops duplicate native siblings after canonical filtering runs', () => {
    const items = dedupeNativeMenuItems([
      item({
        id: 'first-open-terminal',
        label: 'Open in Terminal',
        normalizedVerb: 'openinterminal',
        invokeToken: 'native:first',
      }),
      item({
        id: 'second-open-terminal',
        label: 'Open in Terminal',
        normalizedVerb: 'openinterminal',
        invokeToken: 'native:second',
      }),
    ])

    expect(items.map((entry) => entry.id)).toEqual(['first-open-terminal'])
  })
})
