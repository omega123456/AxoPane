type BreadcrumbSourceSegment = {
  label: string
  path: string
}

export type LayoutSegment = {
  label: string
  fullLabel: string
  path: string
  truncated: boolean
}

export type BreadcrumbLayout = {
  items: LayoutSegment[]
  collapsed: boolean
}

export type BreadcrumbLayoutMeasure = {
  segment: (label: string) => number
  currentSegment?: (label: string) => number
  collapseMarker?: () => number
}

type ComputeBreadcrumbLayoutArgs = {
  segments: BreadcrumbSourceSegment[]
  availableWidth: number
  measure: BreadcrumbLayoutMeasure
  minChars?: number
}

const DEFAULT_MIN_CHARS = 2
const COLLAPSE_MARKER_LABEL = '..'
const BREADCRUMB_BUTTON_PADDING_PX = 16
const BREADCRUMB_CHEVRON_GAP_PX = 4
const BREADCRUMB_CHEVRON_WIDTH_PX = 14
const BREADCRUMB_ITEM_GAP_PX = 4
const BREADCRUMB_SEGMENT_CHROME_PX =
  BREADCRUMB_BUTTON_PADDING_PX +
  BREADCRUMB_CHEVRON_GAP_PX +
  BREADCRUMB_CHEVRON_WIDTH_PX +
  BREADCRUMB_ITEM_GAP_PX
const BREADCRUMB_CURRENT_SEGMENT_CHROME_PX = BREADCRUMB_BUTTON_PADDING_PX
const BREADCRUMB_COLLAPSE_MARKER_CHROME_PX =
  BREADCRUMB_CHEVRON_GAP_PX +
  BREADCRUMB_CHEVRON_WIDTH_PX +
  BREADCRUMB_ITEM_GAP_PX

export function computeBreadcrumbLayout({
  segments,
  availableWidth,
  measure,
  minChars = DEFAULT_MIN_CHARS,
}: ComputeBreadcrumbLayoutArgs): BreadcrumbLayout {
  if (segments.length === 0) {
    return { items: [], collapsed: false }
  }

  if (availableWidth <= 0 || !Number.isFinite(availableWidth)) {
    return createFullLayout(segments)
  }

  const currentSegmentMeasure = measure.currentSegment ?? measure.segment
  const collapseMarkerMeasure = measure.collapseMarker ?? (() => measure.segment(COLLAPSE_MARKER_LABEL))
  const fullLabels = segments.map((segment) => segment.label)
  const fullWidth = sumLayoutWidths(fullLabels, measure.segment, currentSegmentMeasure)
  if (!Number.isFinite(fullWidth)) {
    return createFullLayout(segments)
  }
  if (fullWidth <= availableWidth) {
    return createLayout(segments, fullLabels, false)
  }

  const floorLengths = segments.map((segment) => Math.min(minChars, segment.label.length))
  const collapseMarkerWidth = collapseMarkerMeasure()
  if (!Number.isFinite(collapseMarkerWidth)) {
    return createFullLayout(segments)
  }

  for (let dropped = 1; dropped < segments.length; dropped += 1) {
    const keptSegments = segments.slice(dropped)
    const keptLayout = findShortestFittingLabels({
      segments: keptSegments,
      floorLengths: floorLengths.slice(dropped),
      availableWidth: availableWidth - collapseMarkerWidth,
      segmentMeasure: measure.segment,
      currentSegmentMeasure,
      keepLastFull: true,
    })
    if (keptLayout === 'invalid') {
      return createFullLayout(segments)
    }
    if (keptLayout) {
      return createLayout(keptSegments, keptLayout, true)
    }
  }

  const finalSegment = segments[segments.length - 1]!
  const collapsedLastOnlyLayout = findShortestFittingLabels({
    segments: [finalSegment],
    floorLengths: [floorLengths[floorLengths.length - 1]!],
    availableWidth: availableWidth - collapseMarkerWidth,
    segmentMeasure: measure.segment,
    currentSegmentMeasure,
  })
  if (collapsedLastOnlyLayout === 'invalid') {
    return createFullLayout(segments)
  }
  if (collapsedLastOnlyLayout) {
    return createLayout([finalSegment], collapsedLastOnlyLayout, true)
  }

  const shortenedFullLayout = findShortestFittingLabels({
    segments,
    floorLengths,
    availableWidth,
    segmentMeasure: measure.segment,
    currentSegmentMeasure,
  })
  if (shortenedFullLayout === 'invalid') {
    return createFullLayout(segments)
  }
  if (shortenedFullLayout) {
    return createLayout(segments, shortenedFullLayout, false)
  }

  return createLayout([finalSegment], [finalSegment.label], false)
}

export function createBreadcrumbMeasurer(navElement: HTMLElement | null): BreadcrumbLayoutMeasure {
  if (!navElement) {
    return { segment: () => Number.NaN }
  }

  const canvas = document.createElement('canvas')
  const context = canvas.getContext('2d')
  if (!context) {
    return { segment: () => Number.NaN }
  }

  const font = window.getComputedStyle(navElement).font
  if (font) {
    context.font = font
  }

  return {
    segment: (label) => context.measureText(label).width + BREADCRUMB_SEGMENT_CHROME_PX,
    currentSegment: (label) =>
      context.measureText(label).width + BREADCRUMB_CURRENT_SEGMENT_CHROME_PX,
    collapseMarker: () =>
      context.measureText(COLLAPSE_MARKER_LABEL).width + BREADCRUMB_COLLAPSE_MARKER_CHROME_PX,
  }
}

function createFullLayout(segments: BreadcrumbSourceSegment[]): BreadcrumbLayout {
  return createLayout(
    segments,
    segments.map((segment) => segment.label),
    false,
  )
}

function createLayout(
  segments: BreadcrumbSourceSegment[],
  labels: string[],
  collapsed: boolean,
): BreadcrumbLayout {
  return {
    items: segments.map((segment, index) => ({
      label: labels[index]!,
      fullLabel: segment.label,
      path: segment.path,
      truncated: labels[index] !== segment.label,
    })),
    collapsed,
  }
}

function sumLayoutWidths(
  labels: string[],
  segmentMeasure: (label: string) => number,
  currentSegmentMeasure: (label: string) => number,
) {
  return labels.reduce((totalWidth, label, index) => {
    const isLast = index === labels.length - 1
    const nextWidth = isLast ? currentSegmentMeasure(label) : segmentMeasure(label)
    return totalWidth + nextWidth
  }, 0)
}

function findShortestFittingLabels(args: {
  segments: BreadcrumbSourceSegment[]
  floorLengths: number[]
  availableWidth: number
  segmentMeasure: (label: string) => number
  currentSegmentMeasure: (label: string) => number
  keepLastFull?: boolean
}): string[] | null | 'invalid' {
  const {
    segments,
    floorLengths,
    availableWidth,
    segmentMeasure,
    currentSegmentMeasure,
    keepLastFull = false,
  } = args

  const maxCut = Math.max(
    ...segments.map((segment, index) => {
      const isLockedLast = keepLastFull && index === segments.length - 1
      return isLockedLast ? 0 : segment.label.length - floorLengths[index]!
    }),
  )

  for (let cut = 0; cut <= maxCut; cut += 1) {
    const labels = segments.map((segment, index) => {
      const isLockedLast = keepLastFull && index === segments.length - 1
      if (isLockedLast) {
        return segment.label
      }

      return segment.label.slice(0, Math.max(floorLengths[index]!, segment.label.length - cut))
    })
    const totalWidth = sumLayoutWidths(labels, segmentMeasure, currentSegmentMeasure)
    if (!Number.isFinite(totalWidth)) {
      return 'invalid'
    }
    if (totalWidth <= availableWidth) {
      return expandLabelsToFillAvailableWidth({
        segments,
        labels,
        availableWidth,
        segmentMeasure,
        currentSegmentMeasure,
      })
    }
  }

  return null
}

function expandLabelsToFillAvailableWidth(args: {
  segments: BreadcrumbSourceSegment[]
  labels: string[]
  availableWidth: number
  segmentMeasure: (label: string) => number
  currentSegmentMeasure: (label: string) => number
}) {
  const { segments, labels, availableWidth, segmentMeasure, currentSegmentMeasure } = args
  const expandedLabels = [...labels]

  while (true) {
    let changed = false

    for (let index = segments.length - 1; index >= 0; index -= 1) {
      const fullLabel = segments[index]!.label
      const currentLabel = expandedLabels[index]!
      if (currentLabel.length >= fullLabel.length) {
        continue
      }

      const nextLabels = [...expandedLabels]
      nextLabels[index] = fullLabel.slice(0, currentLabel.length + 1)
      const totalWidth = sumLayoutWidths(nextLabels, segmentMeasure, currentSegmentMeasure)
      if (totalWidth <= availableWidth) {
        expandedLabels[index] = nextLabels[index]!
        changed = true
      }
    }

    if (!changed) {
      return expandedLabels
    }
  }
}
