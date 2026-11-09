const NATURAL_NAME_ORDER: Intl.CollatorOptions = { numeric: true, sensitivity: 'base' }

export function compareNaturally(left: string, right: string) {
  return left.localeCompare(right, undefined, NATURAL_NAME_ORDER)
}
