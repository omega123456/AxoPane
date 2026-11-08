import {
  activeFillClass,
  disclosureChevronClass,
  dropTargetOutline,
  dropTargetRing,
  flippedChevronClass,
  HOVER_FILL,
  HOVER_ON_POINTER,
  menuItemStateClass,
  NO_CLASS,
  spinningClass,
} from '@/lib/class-names'

describe('dropTargetOutline', () => {
  it('outlines only the current drop target', () => {
    expect(dropTargetOutline(true)).toContain('outline-2')
    expect(dropTargetOutline(false)).toBe('')
  })
})

describe('dropTargetRing', () => {
  it('rings only the current drop target', () => {
    expect(dropTargetRing(true)).toBe('ring-2 ring-inset ring-light-accent dark:ring-dark-accent')
    expect(dropTargetRing(false)).toBe(NO_CLASS)
  })
})

describe('activeFillClass', () => {
  it('fills only the active item', () => {
    expect(activeFillClass(true)).toBe(HOVER_FILL)
    expect(activeFillClass(false)).toBe(NO_CLASS)
  })
})

describe('menuItemStateClass', () => {
  it('fills the active item and leaves the rest to pointer hover', () => {
    expect(menuItemStateClass(true)).toBe(HOVER_FILL)
    expect(menuItemStateClass(false)).toBe(HOVER_ON_POINTER)
  })
})

describe('chevron rotation classes', () => {
  it('turns a disclosure chevron a quarter when expanded', () => {
    expect(disclosureChevronClass(true)).toBe('rotate-90')
    expect(disclosureChevronClass(false)).toBe(NO_CLASS)
  })

  it('turns a flipped chevron half way', () => {
    expect(flippedChevronClass(true)).toBe('rotate-180')
    expect(flippedChevronClass(false)).toBe(NO_CLASS)
  })
})

describe('spinningClass', () => {
  it('spins only while busy', () => {
    expect(spinningClass(true)).toBe('animate-spin')
    expect(spinningClass(false)).toBe(NO_CLASS)
  })
})
