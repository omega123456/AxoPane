import { describe, expect, it } from 'vitest'
import { propertiesRow } from '@/lib/context-menu/menu-rows'
import { createPathPropertiesDialogItem } from '@/lib/properties-commands'

describe('propertiesRow', () => {
  it('opens the properties dialog for the given items', () => {
    const item = createPathPropertiesDialogItem('/Users/me')
    expect(propertiesRow({ id: 'properties-me', items: [item] })).toEqual({
      id: 'properties-me',
      kind: 'action',
      label: 'Properties',
      owner: 'app',
      disabled: undefined,
      danger: undefined,
      hidden: undefined,
      strong: undefined,
      shortcut: undefined,
      icon: { kind: 'app', name: 'properties' },
      action: { kind: 'properties', items: [item] },
    })
  })

  it('passes the disabled flag through', () => {
    expect(propertiesRow({ id: 'properties-none', items: [], disabled: true }).disabled).toBe(true)
  })
})
