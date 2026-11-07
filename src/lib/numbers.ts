export const PERCENT_SCALE = 100
export const BYTES_PER_KIBIBYTE = 1024
export const BYTES_PER_MEBIBYTE = BYTES_PER_KIBIBYTE * BYTES_PER_KIBIBYTE
/** JavaScript strings are UTF-16, so each code unit occupies two bytes. */
export const UTF16_BYTES_PER_CODE_UNIT = 2
export const BOTH_SIDES = 2
export const HALF_DIVISOR = 2

/** Bounds `value` to `[minimum, maximum]`; `minimum` wins when the range is empty. */
export function clamp(value: number, minimum: number, maximum: number) {
  return Math.max(minimum, Math.min(value, maximum))
}

export function clampPercent(percent: number) {
  return clamp(percent, 0, PERCENT_SCALE)
}

export function fractionToPercent(fraction: number) {
  return fraction * PERCENT_SCALE
}

export function percentToFraction(percent: number) {
  return percent / PERCENT_SCALE
}

export function utf16ByteWeight(text: string | null) {
  return text == null ? 1 : text.length * UTF16_BYTES_PER_CODE_UNIT
}
