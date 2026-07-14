export const paneViewModes = ['details', 'icons', 'thumbnails'] as const

export type PaneViewMode = (typeof paneViewModes)[number]

export type PaneViewMetadata = {
  label: string
  icon: 'List' | 'Grid2X2' | 'Images'
}

export const paneViewMetadata: Record<PaneViewMode, PaneViewMetadata> = {
  details: { label: 'Details', icon: 'List' },
  icons: { label: 'Icons', icon: 'Grid2X2' },
  thumbnails: { label: 'Large thumbnails', icon: 'Images' },
}

export function isPaneViewMode(value: unknown): value is PaneViewMode {
  return typeof value === 'string' && paneViewModes.includes(value as PaneViewMode)
}

export function resolvePaneViewMode(value: unknown, fallback: PaneViewMode): PaneViewMode {
  return isPaneViewMode(value) ? value : fallback
}
