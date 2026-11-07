import {
  cardHighlightClass,
  cutFadeClass,
  itemCountLabel,
  rowDropTargetClass,
  rowHighlightClass,
} from '@/components/pane/entry-classes'

const PLAIN = { isActivePane: true, isSelected: false, isFocused: false }

describe('cutFadeClass', () => {
  it('fades only cut entries', () => {
    expect(cutFadeClass(true)).toBe('opacity-50')
    expect(cutFadeClass(false)).toBe('')
  })
})

describe('rowHighlightClass', () => {
  it('uses the accent tint in the active pane and the neutral fill elsewhere', () => {
    expect(rowHighlightClass({ ...PLAIN, isSelected: true })).toContain('bg-light-tint')
    expect(rowHighlightClass({ ...PLAIN, isSelected: true, isActivePane: false })).toContain(
      'bg-light-neutral-selection',
    )
    expect(rowHighlightClass(PLAIN)).toContain('hover:bg-light-hover')
  })

  it('draws the cursor ring only for the focused row of the active pane', () => {
    expect(rowHighlightClass({ ...PLAIN, isFocused: true })).toContain('ring-1')
    expect(rowHighlightClass({ ...PLAIN, isFocused: true, isActivePane: false })).not.toContain(
      'ring-1',
    )
  })
})

describe('rowDropTargetClass', () => {
  it('rings and tints only the hovered drop target', () => {
    expect(rowDropTargetClass(true)).toContain('ring-2')
    expect(rowDropTargetClass(false)).toBe('')
  })
})

describe('cardHighlightClass', () => {
  it('fills selected and drop-target cards', () => {
    expect(cardHighlightClass({ ...PLAIN, isSelected: true, isDropTarget: false })).toContain(
      'border-light-accent',
    )
    expect(cardHighlightClass({ ...PLAIN, isDropTarget: true, isActivePane: false })).toContain(
      'bg-light-neutral-selection',
    )
    expect(cardHighlightClass({ ...PLAIN, isDropTarget: false })).toContain('hover:bg-light-hover')
  })

  it('omits the cursor ring on a selected card', () => {
    expect(cardHighlightClass({ ...PLAIN, isFocused: true, isDropTarget: false })).toContain(
      'ring-1',
    )
    expect(
      cardHighlightClass({ ...PLAIN, isFocused: true, isSelected: true, isDropTarget: false }),
    ).not.toContain('ring-1')
  })
})

describe('itemCountLabel', () => {
  it('pluralises counted folders and dashes uncounted ones', () => {
    expect(itemCountLabel(1)).toBe('1 item')
    expect(itemCountLabel(3)).toBe('3 items')
    expect(itemCountLabel(null)).toBe('—')
  })
})
