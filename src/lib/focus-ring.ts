/**
 * Two-tone keyboard focus ring (Fluent FocusStrokeColorOuter/Inner): a 2px
 * outer ring in the primary text colour over a 1px inner ring in the opposite
 * colour, so it clears 3:1 on every surface in both themes.
 */
export const FOCUS_RING =
  'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-light-focus-outer focus-visible:ring-offset-1 focus-visible:ring-offset-light-focus-inner dark:focus-visible:ring-dark-focus-outer dark:focus-visible:ring-offset-dark-focus-inner'

/** Inset variant for rows, menu items and nav entries; inset rings take no offset. */
export const FOCUS_RING_INSET =
  'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-light-focus-outer dark:focus-visible:ring-dark-focus-outer'
