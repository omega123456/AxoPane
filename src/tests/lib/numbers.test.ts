import {
  clamp,
  clampPercent,
  fractionToPercent,
  percentToFraction,
  utf16ByteWeight,
  wrapIndex,
} from '@/lib/numbers'

describe('numbers', () => {
  it('clamps into an inclusive range', () => {
    expect(clamp(-5, 0, 10)).toBe(0)
    expect(clamp(15, 0, 10)).toBe(10)
    expect(clamp(7, 0, 10)).toBe(7)
  })

  it('clamps percentages to 0–100', () => {
    expect(clampPercent(-1)).toBe(0)
    expect(clampPercent(140)).toBe(100)
    expect(clampPercent(42.5)).toBe(42.5)
  })

  it('converts between fractions and percentages', () => {
    expect(fractionToPercent(0.25)).toBe(25)
    expect(percentToFraction(125)).toBe(1.25)
  })

  it('weighs UTF-16 strings at two bytes per code unit and null at one', () => {
    expect(utf16ByteWeight(null)).toBe(1)
    expect(utf16ByteWeight('abcd')).toBe(8)
  })

  it('wraps indexes past either end back into range', () => {
    expect(wrapIndex(2, 5)).toBe(2)
    expect(wrapIndex(5, 5)).toBe(0)
    expect(wrapIndex(-1, 5)).toBe(4)
    expect(wrapIndex(-6, 5)).toBe(4)
    expect(wrapIndex(12, 5)).toBe(2)
  })
})
